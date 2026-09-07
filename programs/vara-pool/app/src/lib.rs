//! Vale Protocol kVARA pool: liquid staking for native VARA.
//!
//! The pool program *is* the receipt token (kVARA): its `Vft` service is the standard
//! fungible token surface over share balances, and its `Pool` service moves VARA in
//! and out. Staking is payable: the VARA attached to `stake` is the deposit.
//!
//! Accounting is rate based (ERC-4626 shape). `rate` is VARA per kVARA scaled by `1e18`,
//! starts at exactly `1.0` and grows at the configured APY, checkpointed on every state
//! changing call. Payouts come from the pool's own VARA reserve: deposits plus rewards
//! sent through `fund_rewards`. A payout larger than the reserve is refused, nothing is
//! burned, and the position stays intact.
//!
//! Everything here is synchronous: value moves with the message itself, so there are no
//! cross-program calls and no partial states to compensate.

#![no_std]

use core::cell::RefCell;
use sails_rs::{collections::BTreeMap, gstd::CommandReply, prelude::*};

/// Route index of the `vft` service inside the program (first service => 1).
const VFT_ROUTE_IDX: u8 = 1;
/// Rate scale: 1e18.
pub const SCALE: U256 = U256([1_000_000_000_000_000_000u64, 0, 0, 0]);
pub const BPS: u64 = 10_000;
pub const YEAR_SECS: u64 = 31_536_000;
/// Smallest stake: 0.01 VARA (12 decimals), so share rounding can never produce zero.
pub const MIN_STAKE: u128 = 10_000_000_000;
pub const MAX_APY_BPS: u32 = 100_000; // 1000% (demo ceiling)
pub const MAX_FEE_BPS: u32 = 1_000; // 10%
/// Longest session a user may register.
pub const MAX_SESSION_SECS: u64 = 30 * 86_400;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolError {
    Paused,
    ZeroAddress,
    ZeroAmount,
    BelowMinimum { min: U256 },
    InsufficientShares,
    InsufficientBalance,
    InsufficientAllowance,
    /// The pool's VARA reserve cannot cover this payout yet; nothing changed.
    InsufficientReserve { available: U256 },
    Unauthorized,
    Overflow,
    BadConfig,
    UnbondNotFound,
    UnbondNotReady { claimable_at: u64 },
    /// The runtime refused the value transfer; the position was restored.
    TransferFailed(String),
    /// The session key acting for the owner has expired.
    SessionExpired,
    /// The session key acting for the owner may not perform this action.
    SessionNotAllowed,
    /// Session key, duration or action list is invalid.
    BadSession,
}

/// Actions a session key may perform for its owner.
#[sails_rs::sails_type]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Stake,
    Unstake,
    Unbond,
    Claim,
}

/// A session key registered by an owner: messages signed by `key` act for the owner until
/// `expires_at`. Payouts always go to the owner, never to the key.
#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub key: ActorId,
    pub expires_at: u64,
    pub actions: Vec<SessionAction>,
}

#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unbond {
    pub id: u64,
    pub shares: U256,
    pub assets: U256,
    pub requested_at: u64,
    pub claimable_at: u64,
}

#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnstakePreview {
    pub assets: U256,
    pub fee: U256,
    pub net: U256,
}

#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub shares: U256,
    pub assets: U256,
}

#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolInfo {
    pub admin: ActorId,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    /// VARA per kVARA scaled by 1e18, projected to now.
    pub rate: U256,
    pub apy_bps: u32,
    pub instant_fee_bps: u32,
    pub unbond_period_secs: u64,
    pub total_shares: U256,
    /// VARA owed to share holders at the projected rate.
    pub total_assets: U256,
    /// VARA the pool physically holds for payouts (stakes plus funded rewards).
    pub reserve: U256,
    pub fees_accrued: U256,
    pub unbonding_total: U256,
    pub min_stake: U256,
    pub paused: bool,
    pub last_accrual_at: u64,
}

// ---------------------------------------------------------------------------
// State and pure transitions (unit tested on the host)
// ---------------------------------------------------------------------------

pub struct PoolState {
    pub admin: ActorId,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_shares: U256,
    pub balances: BTreeMap<ActorId, U256>,
    pub allowances: BTreeMap<(ActorId, ActorId), U256>,
    pub rate: U256,
    pub last_accrual_ms: u64,
    pub apy_bps: u32,
    pub instant_fee_bps: u32,
    pub unbond_period_ms: u64,
    /// VARA held for payouts, in base units.
    pub reserve: U256,
    pub fees_accrued: U256,
    pub unbonding_total: U256,
    pub paused: bool,
    pub unbonds: BTreeMap<ActorId, Vec<Unbond>>,
    pub next_unbond_id: u64,
    /// owner -> session
    pub sessions: BTreeMap<ActorId, Session>,
    /// session key -> owner
    pub session_keys: BTreeMap<ActorId, ActorId>,
}

impl PoolState {
    pub fn new(admin: ActorId, name: String, symbol: String, decimals: u8, apy_bps: u32, instant_fee_bps: u32, unbond_period_secs: u64, now_ms: u64) -> Self {
        Self {
            admin,
            name,
            symbol,
            decimals,
            total_shares: U256::zero(),
            balances: BTreeMap::new(),
            allowances: BTreeMap::new(),
            rate: SCALE,
            last_accrual_ms: now_ms,
            apy_bps,
            instant_fee_bps,
            unbond_period_ms: unbond_period_secs.saturating_mul(1000),
            reserve: U256::zero(),
            fees_accrued: U256::zero(),
            unbonding_total: U256::zero(),
            paused: false,
            unbonds: BTreeMap::new(),
            next_unbond_id: 1,
            sessions: BTreeMap::new(),
            session_keys: BTreeMap::new(),
        }
    }

    // --- rate ---------------------------------------------------------------

    /// Rate projected to `now_ms` without mutating.
    pub fn rate_at(&self, now_ms: u64) -> U256 {
        if now_ms <= self.last_accrual_ms || self.apy_bps == 0 { return self.rate; }
        let dt = U256::from((now_ms - self.last_accrual_ms) / 1000);
        let growth = self.rate.saturating_mul(U256::from(self.apy_bps)).saturating_mul(dt) / U256::from(BPS * YEAR_SECS);
        self.rate.saturating_add(growth)
    }

    /// Checkpoint the rate at `now_ms`. Returns true if it moved.
    pub fn accrue(&mut self, now_ms: u64) -> bool {
        let r = self.rate_at(now_ms);
        // Only move the clock forward on whole seconds so sub-second dust is not lost.
        if now_ms > self.last_accrual_ms {
            let secs = (now_ms - self.last_accrual_ms) / 1000;
            self.last_accrual_ms += secs * 1000;
        }
        if r != self.rate { self.rate = r; true } else { false }
    }

    pub fn shares_for(&self, assets: U256) -> Result<U256, PoolError> {
        assets.checked_mul(SCALE).ok_or(PoolError::Overflow).map(|v| v / self.rate)
    }

    pub fn assets_for(&self, shares: U256) -> Result<U256, PoolError> {
        shares.checked_mul(self.rate).ok_or(PoolError::Overflow).map(|v| v / SCALE)
    }

    pub fn total_assets(&self) -> U256 {
        self.assets_for(self.total_shares).unwrap_or(U256::MAX)
    }

    pub fn preview_unstake(&self, shares: U256) -> Result<UnstakePreview, PoolError> {
        let assets = self.assets_for(shares)?;
        let fee = assets.saturating_mul(U256::from(self.instant_fee_bps)) / U256::from(BPS);
        Ok(UnstakePreview { assets, fee, net: assets - fee })
    }

    // --- shares (receipt token) ---------------------------------------------

    pub fn balance_of(&self, who: &ActorId) -> U256 {
        self.balances.get(who).copied().unwrap_or_default()
    }

    pub fn allowance(&self, owner: &ActorId, spender: &ActorId) -> U256 {
        self.allowances.get(&(*owner, *spender)).copied().unwrap_or_default()
    }

    fn ensure_live(&self) -> Result<(), PoolError> {
        if self.paused { Err(PoolError::Paused) } else { Ok(()) }
    }

    fn credit(&mut self, to: ActorId, value: U256) -> Result<(), PoolError> {
        let new = self.balance_of(&to).checked_add(value).ok_or(PoolError::Overflow)?;
        self.balances.insert(to, new);
        Ok(())
    }

    fn debit(&mut self, from: ActorId, value: U256) -> Result<(), PoolError> {
        let new = self.balance_of(&from).checked_sub(value).ok_or(PoolError::InsufficientBalance)?;
        if new.is_zero() { self.balances.remove(&from); } else { self.balances.insert(from, new); }
        Ok(())
    }

    pub fn transfer_shares(&mut self, from: ActorId, to: ActorId, value: U256) -> Result<(), PoolError> {
        self.ensure_live()?;
        if to.is_zero() { return Err(PoolError::ZeroAddress); }
        if value.is_zero() { return Err(PoolError::ZeroAmount); }
        self.debit(from, value)?;
        self.credit(to, value)
    }

    pub fn approve_shares(&mut self, owner: ActorId, spender: ActorId, value: U256) -> Result<(), PoolError> {
        self.ensure_live()?;
        if spender.is_zero() { return Err(PoolError::ZeroAddress); }
        if value.is_zero() { self.allowances.remove(&(owner, spender)); } else { self.allowances.insert((owner, spender), value); }
        Ok(())
    }

    pub fn spend_allowance(&mut self, owner: ActorId, spender: ActorId, value: U256) -> Result<(), PoolError> {
        let current = self.allowance(&owner, &spender);
        if current == U256::MAX { return Ok(()); }
        let rest = current.checked_sub(value).ok_or(PoolError::InsufficientAllowance)?;
        if rest.is_zero() { self.allowances.remove(&(owner, spender)); } else { self.allowances.insert((owner, spender), rest); }
        Ok(())
    }

    pub fn mint_shares(&mut self, to: ActorId, value: U256) -> Result<(), PoolError> {
        let supply = self.total_shares.checked_add(value).ok_or(PoolError::Overflow)?;
        self.credit(to, value)?;
        self.total_shares = supply;
        Ok(())
    }

    pub fn burn_shares(&mut self, from: ActorId, value: U256) -> Result<(), PoolError> {
        if value.is_zero() { return Err(PoolError::ZeroAmount); }
        self.debit(from, value).map_err(|_| PoolError::InsufficientShares)?;
        self.total_shares = self.total_shares.checked_sub(value).ok_or(PoolError::Overflow)?;
        Ok(())
    }

    // --- pool flows ---------------------------------------------------------

    /// Stake `assets` of VARA that arrived with the message. Returns the shares minted.
    pub fn stake(&mut self, owner: ActorId, assets: U256) -> Result<U256, PoolError> {
        self.ensure_live()?;
        if assets.is_zero() { return Err(PoolError::ZeroAmount); }
        if assets < U256::from(MIN_STAKE) { return Err(PoolError::BelowMinimum { min: U256::from(MIN_STAKE) }); }
        let shares = self.shares_for(assets)?;
        if shares.is_zero() { return Err(PoolError::BelowMinimum { min: U256::from(MIN_STAKE) }); }
        self.mint_shares(owner, shares)?;
        self.reserve = self.reserve.checked_add(assets).ok_or(PoolError::Overflow)?;
        Ok(shares)
    }

    /// Burn shares for an instant exit and take the payout from the reserve.
    /// Refuses (without changes) when the reserve cannot cover the net amount.
    pub fn unstake(&mut self, owner: ActorId, shares: U256) -> Result<UnstakePreview, PoolError> {
        self.ensure_live()?;
        if shares.is_zero() { return Err(PoolError::ZeroAmount); }
        if self.balance_of(&owner) < shares { return Err(PoolError::InsufficientShares); }
        let p = self.preview_unstake(shares)?;
        if p.net.is_zero() { return Err(PoolError::ZeroAmount); }
        if p.net > self.reserve { return Err(PoolError::InsufficientReserve { available: self.reserve }); }
        self.burn_shares(owner, shares)?;
        self.fees_accrued = self.fees_accrued.saturating_add(p.fee);
        self.reserve -= p.net;
        Ok(p)
    }

    /// Undo `unstake` after a failed payout.
    pub fn revert_unstake(&mut self, owner: ActorId, shares: U256, p: &UnstakePreview) {
        let _ = self.mint_shares(owner, shares);
        self.fees_accrued = self.fees_accrued.saturating_sub(p.fee);
        self.reserve = self.reserve.saturating_add(p.net);
    }

    /// Rewards or top-ups sent to the pool grow the reserve.
    pub fn fund(&mut self, assets: U256) -> Result<(), PoolError> {
        if assets.is_zero() { return Err(PoolError::ZeroAmount); }
        self.reserve = self.reserve.checked_add(assets).ok_or(PoolError::Overflow)?;
        Ok(())
    }

    pub fn request_unbond(&mut self, owner: ActorId, shares: U256, now_ms: u64) -> Result<Unbond, PoolError> {
        self.ensure_live()?;
        if shares.is_zero() { return Err(PoolError::ZeroAmount); }
        if self.balance_of(&owner) < shares { return Err(PoolError::InsufficientShares); }
        let assets = self.assets_for(shares)?;
        if assets.is_zero() { return Err(PoolError::ZeroAmount); }
        self.burn_shares(owner, shares)?;
        self.unbonding_total = self.unbonding_total.checked_add(assets).ok_or(PoolError::Overflow)?;
        let entry = Unbond { id: self.next_unbond_id, shares, assets, requested_at: now_ms, claimable_at: now_ms.saturating_add(self.unbond_period_ms) };
        self.next_unbond_id += 1;
        self.unbonds.entry(owner).or_default().push(entry.clone());
        Ok(entry)
    }

    /// Remove a claimable unbond entry and take its VARA from the reserve.
    pub fn take_unbond(&mut self, owner: ActorId, id: u64, now_ms: u64) -> Result<Unbond, PoolError> {
        self.ensure_live()?;
        let list = self.unbonds.get(&owner).ok_or(PoolError::UnbondNotFound)?;
        let idx = list.iter().position(|u| u.id == id).ok_or(PoolError::UnbondNotFound)?;
        if now_ms < list[idx].claimable_at { return Err(PoolError::UnbondNotReady { claimable_at: list[idx].claimable_at }); }
        if list[idx].assets > self.reserve { return Err(PoolError::InsufficientReserve { available: self.reserve }); }
        let list = self.unbonds.get_mut(&owner).ok_or(PoolError::UnbondNotFound)?;
        let entry = list.remove(idx);
        if list.is_empty() { self.unbonds.remove(&owner); }
        self.unbonding_total = self.unbonding_total.saturating_sub(entry.assets);
        self.reserve -= entry.assets;
        Ok(entry)
    }

    /// Put an unbond entry back after a failed payout.
    pub fn restore_unbond(&mut self, owner: ActorId, entry: Unbond) {
        self.unbonding_total = self.unbonding_total.saturating_add(entry.assets);
        self.reserve = self.reserve.saturating_add(entry.assets);
        let list = self.unbonds.entry(owner).or_default();
        list.push(entry);
        list.sort_by_key(|u| u.id);
    }

    pub fn unbonds_of(&self, owner: &ActorId) -> Vec<Unbond> {
        self.unbonds.get(owner).cloned().unwrap_or_default()
    }

    fn ensure_admin(&self, who: ActorId) -> Result<(), PoolError> {
        if self.admin == who { Ok(()) } else { Err(PoolError::Unauthorized) }
    }

    // --- sessions ------------------------------------------------------------

    /// Who a message acts for: the sender itself, or the owner of the session key it holds.
    pub fn actor_for(&self, sender: ActorId, action: SessionAction, now_ms: u64) -> Result<ActorId, PoolError> {
        let Some(owner) = self.session_keys.get(&sender) else { return Ok(sender) };
        let session = self.sessions.get(owner).ok_or(PoolError::SessionExpired)?;
        if now_ms >= session.expires_at { return Err(PoolError::SessionExpired); }
        if !session.actions.contains(&action) { return Err(PoolError::SessionNotAllowed); }
        Ok(*owner)
    }

    pub fn create_session(&mut self, owner: ActorId, key: ActorId, duration_secs: u64, actions: Vec<SessionAction>, now_ms: u64) -> Result<Session, PoolError> {
        if key.is_zero() || key == owner || duration_secs == 0 || duration_secs > MAX_SESSION_SECS || actions.is_empty() { return Err(PoolError::BadSession); }
        // A key may serve one owner; an owner may hold one session.
        if let Some(other) = self.session_keys.get(&key) { if *other != owner { return Err(PoolError::BadSession); } }
        if let Some(old) = self.sessions.remove(&owner) { self.session_keys.remove(&old.key); }
        let session = Session { key, expires_at: now_ms.saturating_add(duration_secs.saturating_mul(1000)), actions };
        self.sessions.insert(owner, session.clone());
        self.session_keys.insert(key, owner);
        Ok(session)
    }

    pub fn revoke_session(&mut self, owner: ActorId) -> Option<Session> {
        let old = self.sessions.remove(&owner)?;
        self.session_keys.remove(&old.key);
        Some(old)
    }

    pub fn info(&self, now_ms: u64) -> PoolInfo {
        let rate = self.rate_at(now_ms);
        let total_assets = self.total_shares.checked_mul(rate).map(|v| v / SCALE).unwrap_or(U256::MAX);
        PoolInfo {
            admin: self.admin,
            name: self.name.clone(),
            symbol: self.symbol.clone(),
            decimals: self.decimals,
            rate,
            apy_bps: self.apy_bps,
            instant_fee_bps: self.instant_fee_bps,
            unbond_period_secs: self.unbond_period_ms / 1000,
            total_shares: self.total_shares,
            total_assets,
            reserve: self.reserve,
            fees_accrued: self.fees_accrued,
            unbonding_total: self.unbonding_total,
            min_stake: U256::from(MIN_STAKE),
            paused: self.paused,
            last_accrual_at: self.last_accrual_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Vft service: the receipt token
// ---------------------------------------------------------------------------

#[sails_rs::sails_type]
#[sails_rs::event]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VftEvent {
    Approval { owner: ActorId, spender: ActorId, value: U256 },
    Transfer { from: ActorId, to: ActorId, value: U256 },
}

pub struct Vft<'a> {
    state: &'a RefCell<PoolState>,
}

impl<'a> Vft<'a> {
    pub fn new(state: &'a RefCell<PoolState>) -> Self {
        Self { state }
    }
}

#[sails_rs::service(events = VftEvent)]
impl Vft<'_> {
    #[export(unwrap_result)]
    pub fn approve(&mut self, spender: ActorId, value: U256) -> Result<bool, PoolError> {
        let owner = Syscall::message_source();
        self.state.borrow_mut().approve_shares(owner, spender, value)?;
        self.emit_event(VftEvent::Approval { owner, spender, value }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn transfer(&mut self, to: ActorId, value: U256) -> Result<bool, PoolError> {
        let from = Syscall::message_source();
        self.state.borrow_mut().transfer_shares(from, to, value)?;
        self.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn transfer_from(&mut self, from: ActorId, to: ActorId, value: U256) -> Result<bool, PoolError> {
        let spender = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            if spender != from { s.spend_allowance(from, spender, value)?; }
            s.transfer_shares(from, to, value)?;
        }
        self.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn allowance(&self, owner: ActorId, spender: ActorId) -> U256 {
        self.state.borrow().allowance(&owner, &spender)
    }

    #[export]
    pub fn balance_of(&self, account: ActorId) -> U256 {
        self.state.borrow().balance_of(&account)
    }

    #[export]
    pub fn total_supply(&self) -> U256 {
        self.state.borrow().total_shares
    }

    #[export]
    pub fn name(&self) -> String {
        self.state.borrow().name.clone()
    }

    #[export]
    pub fn symbol(&self) -> String {
        self.state.borrow().symbol.clone()
    }

    #[export]
    pub fn decimals(&self) -> u8 {
        self.state.borrow().decimals
    }
}

fn emit_vft_transfer(state: &RefCell<PoolState>, from: ActorId, to: ActorId, value: U256) {
    use sails_rs::gstd::services::Service as _;
    let vft = Vft::new(state).expose(VFT_ROUTE_IDX);
    vft.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
}

// ---------------------------------------------------------------------------
// Pool service
// ---------------------------------------------------------------------------

#[sails_rs::sails_type]
#[sails_rs::event]
#[derive(Debug, Clone, PartialEq, Eq)]
// Alphabetical: the IDL sorts events, the interface hash uses declaration order.
pub enum PoolEvent {
    Accrued { rate: U256, total_assets: U256 },
    AdminTransferred { from: ActorId, to: ActorId },
    Claimed { owner: ActorId, id: u64, assets: U256 },
    ConfigChanged { apy_bps: u32, instant_fee_bps: u32, unbond_period_secs: u64 },
    FeesCollected { to: ActorId, assets: U256 },
    Paused { by: ActorId },
    Resumed { by: ActorId },
    RewardsFunded { from: ActorId, assets: U256, reserve: U256 },
    SessionCreated { owner: ActorId, key: ActorId, expires_at: u64 },
    SessionRevoked { owner: ActorId },
    Staked { owner: ActorId, assets: U256, shares: U256, rate: U256 },
    UnbondRequested { owner: ActorId, id: u64, shares: U256, assets: U256, claimable_at: u64 },
    Unstaked { owner: ActorId, shares: U256, assets: U256, fee: U256, rate: U256 },
}

pub struct Pool<'a> {
    state: &'a RefCell<PoolState>,
}

impl<'a> Pool<'a> {
    pub fn new(state: &'a RefCell<PoolState>) -> Self {
        Self { state }
    }
}

fn now_ms() -> u64 {
    Syscall::block_timestamp()
}

/// U256 amounts in this program never exceed the VARA supply, so the narrowing is exact.
fn as_value(v: U256) -> u128 {
    if v > U256::from(u128::MAX) { u128::MAX } else { v.low_u128() }
}

/// Send `assets` of VARA to `to`. The gas limit is an explicit zero: a user message below the
/// mailbox threshold is not parked in the mailbox, its value lands on the account at once.
fn pay_out(to: ActorId, assets: U256) -> Result<(), PoolError> {
    sails_rs::gstd::msg::send_bytes_with_gas(to, [], 0, as_value(assets)).map(|_| ()).map_err(|e| PoolError::TransferFailed(format!("{e:?}")))
}

#[sails_rs::service(events = PoolEvent)]
impl Pool<'_> {
    // Stake the VARA attached to the message. Returns the kVARA minted. On any error the
    // attached value is sent back with the reply.
    #[export]
    pub fn stake(&mut self) -> CommandReply<Result<U256, PoolError>> {
        let value = Syscall::message_value();
        let assets = U256::from(value);
        let outcome = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            s.actor_for(Syscall::message_source(), SessionAction::Stake, now).and_then(|owner| {
                s.accrue(now);
                s.stake(owner, assets).map(|shares| (owner, shares, s.rate))
            })
        };
        match outcome {
            Ok((owner, shares, rate)) => {
                emit_vft_transfer(self.state, ActorId::zero(), owner, shares);
                self.emit_event(PoolEvent::Staked { owner, assets, shares, rate }).expect("event");
                CommandReply::new(Ok(shares))
            }
            Err(e) => CommandReply::new(Err(e)).with_value(value),
        }
    }

    // Instant exit: burn `shares`, receive VARA at the current rate minus the instant fee.
    #[export]
    pub fn unstake(&mut self, shares: U256) -> Result<UnstakePreview, PoolError> {
        let (owner, preview, rate) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Unstake, now)?;
            s.accrue(now);
            let p = s.unstake(owner, shares)?;
            (owner, p, s.rate)
        };
        match pay_out(owner, preview.net) {
            Ok(()) => {
                emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
                self.emit_event(PoolEvent::Unstaked { owner, shares, assets: preview.assets, fee: preview.fee, rate }).expect("event");
                Ok(preview)
            }
            Err(e) => {
                self.state.borrow_mut().revert_unstake(owner, shares, &preview);
                Err(e)
            }
        }
    }

    // Timed exit: burn `shares` now, lock VARA at the current rate, claim after the unbond period.
    #[export]
    pub fn request_unbond(&mut self, shares: U256) -> Result<Unbond, PoolError> {
        let (owner, entry) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Unbond, now)?;
            s.accrue(now);
            (owner, s.request_unbond(owner, shares, now)?)
        };
        emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
        self.emit_event(PoolEvent::UnbondRequested { owner, id: entry.id, shares, assets: entry.assets, claimable_at: entry.claimable_at }).expect("event");
        Ok(entry)
    }

    // Claim a matured unbond entry. Returns the VARA paid.
    #[export]
    pub fn claim(&mut self, id: u64) -> Result<U256, PoolError> {
        let (owner, entry) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Claim, now)?;
            s.accrue(now);
            (owner, s.take_unbond(owner, id, now)?)
        };
        match pay_out(owner, entry.assets) {
            Ok(()) => {
                self.emit_event(PoolEvent::Claimed { owner, id, assets: entry.assets }).expect("event");
                Ok(entry.assets)
            }
            Err(e) => {
                self.state.borrow_mut().restore_unbond(owner, entry);
                Err(e)
            }
        }
    }

    // Add the attached VARA to the payout reserve. Anyone may fund rewards.
    #[export]
    pub fn fund_rewards(&mut self) -> CommandReply<Result<U256, PoolError>> {
        let value = Syscall::message_value();
        let from = Syscall::message_source();
        let assets = U256::from(value);
        let outcome = {
            let mut s = self.state.borrow_mut();
            s.accrue(now_ms());
            s.fund(assets).map(|_| s.reserve)
        };
        match outcome {
            Ok(reserve) => {
                self.emit_event(PoolEvent::RewardsFunded { from, assets, reserve }).expect("event");
                CommandReply::new(Ok(reserve))
            }
            Err(e) => CommandReply::new(Err(e)).with_value(value),
        }
    }

    // Checkpoint the rate. Anyone may call it.
    #[export]
    pub fn accrue(&mut self) -> U256 {
        let (rate, total_assets, moved) = {
            let mut s = self.state.borrow_mut();
            let moved = s.accrue(now_ms());
            (s.rate, s.total_assets(), moved)
        };
        if moved { self.emit_event(PoolEvent::Accrued { rate, total_assets }).expect("event"); }
        rate
    }

    // --- admin ---------------------------------------------------------------

    #[export]
    pub fn set_config(&mut self, apy_bps: u32, instant_fee_bps: u32, unbond_period_secs: u64) -> Result<bool, PoolError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            if apy_bps > MAX_APY_BPS || instant_fee_bps > MAX_FEE_BPS { return Err(PoolError::BadConfig); }
            // Settle the old APY up to now before switching.
            s.accrue(now_ms());
            s.apy_bps = apy_bps;
            s.instant_fee_bps = instant_fee_bps;
            s.unbond_period_ms = unbond_period_secs.saturating_mul(1000);
        }
        self.emit_event(PoolEvent::ConfigChanged { apy_bps, instant_fee_bps, unbond_period_secs }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn pause(&mut self) -> Result<bool, PoolError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.paused = true;
        }
        self.emit_event(PoolEvent::Paused { by: caller }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn resume(&mut self) -> Result<bool, PoolError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.paused = false;
        }
        self.emit_event(PoolEvent::Resumed { by: caller }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn transfer_admin(&mut self, to: ActorId) -> Result<bool, PoolError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            if to.is_zero() { return Err(PoolError::ZeroAddress); }
            s.admin = to;
        }
        self.emit_event(PoolEvent::AdminTransferred { from: caller, to }).expect("event");
        Ok(true)
    }

    // Send accrued instant-exit fees to `to`. Fees are paid from the reserve.
    #[export]
    pub fn collect_fees(&mut self, to: ActorId) -> Result<U256, PoolError> {
        let caller = Syscall::message_source();
        let amount = {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            if to.is_zero() { return Err(PoolError::ZeroAddress); }
            let amount = s.fees_accrued;
            if amount.is_zero() { return Err(PoolError::ZeroAmount); }
            if amount > s.reserve { return Err(PoolError::InsufficientReserve { available: s.reserve }); }
            s.fees_accrued = U256::zero();
            s.reserve -= amount;
            amount
        };
        match pay_out(to, amount) {
            Ok(()) => {
                self.emit_event(PoolEvent::FeesCollected { to, assets: amount }).expect("event");
                Ok(amount)
            }
            Err(e) => {
                let mut s = self.state.borrow_mut();
                s.fees_accrued = amount;
                s.reserve = s.reserve.saturating_add(amount);
                Err(e)
            }
        }
    }

    // --- sessions --------------------------------------------------------------

    // Register `key` to act for the caller for `duration_secs` (max 30 days), limited to `actions`.
    // Replaces any existing session of the caller.
    #[export]
    pub fn create_session(&mut self, key: ActorId, duration_secs: u64, actions: Vec<SessionAction>) -> Result<Session, PoolError> {
        let owner = Syscall::message_source();
        let session = self.state.borrow_mut().create_session(owner, key, duration_secs, actions, now_ms())?;
        self.emit_event(PoolEvent::SessionCreated { owner, key, expires_at: session.expires_at }).expect("event");
        Ok(session)
    }

    // Drop the caller's session key, if any. Returns whether one existed.
    #[export]
    pub fn revoke_session(&mut self) -> bool {
        let owner = Syscall::message_source();
        let had = self.state.borrow_mut().revoke_session(owner).is_some();
        if had { self.emit_event(PoolEvent::SessionRevoked { owner }).expect("event"); }
        had
    }

    #[export]
    pub fn session(&self, owner: ActorId) -> Option<Session> {
        let s = self.state.borrow();
        s.sessions.get(&owner).filter(|x| now_ms() < x.expires_at).cloned()
    }

    #[export]
    pub fn session_owner(&self, key: ActorId) -> Option<ActorId> {
        let s = self.state.borrow();
        let owner = s.session_keys.get(&key)?;
        s.sessions.get(owner).filter(|x| now_ms() < x.expires_at).map(|_| *owner)
    }

    // --- queries -------------------------------------------------------------

    #[export]
    pub fn info(&self) -> PoolInfo {
        self.state.borrow().info(now_ms())
    }

    // VARA per kVARA scaled by 1e18, projected to the current block.
    #[export]
    pub fn rate(&self) -> U256 {
        self.state.borrow().rate_at(now_ms())
    }

    #[export]
    pub fn preview_stake(&self, assets: U256) -> U256 {
        let s = self.state.borrow();
        let rate = s.rate_at(now_ms());
        assets.checked_mul(SCALE).map(|v| v / rate).unwrap_or(U256::zero())
    }

    #[export]
    pub fn preview_unstake(&self, shares: U256) -> UnstakePreview {
        let s = self.state.borrow();
        let rate = s.rate_at(now_ms());
        let assets = shares.checked_mul(rate).map(|v| v / SCALE).unwrap_or(U256::zero());
        let fee = assets.saturating_mul(U256::from(s.instant_fee_bps)) / U256::from(BPS);
        UnstakePreview { assets, fee, net: assets - fee }
    }

    #[export]
    pub fn position(&self, account: ActorId) -> Position {
        let s = self.state.borrow();
        let shares = s.balance_of(&account);
        let rate = s.rate_at(now_ms());
        let assets = shares.checked_mul(rate).map(|v| v / SCALE).unwrap_or(U256::zero());
        Position { shares, assets }
    }

    #[export]
    pub fn unbonds(&self, account: ActorId) -> Vec<Unbond> {
        self.state.borrow().unbonds_of(&account)
    }
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

pub struct Program {
    state: RefCell<PoolState>,
}

#[sails_rs::program]
impl Program {
    // Deploy the kVARA pool. The deployer becomes admin.
    pub fn new(name: String, symbol: String, decimals: u8, apy_bps: u32, instant_fee_bps: u32, unbond_period_secs: u64) -> Self {
        let state = PoolState::new(Syscall::message_source(), name, symbol, decimals, apy_bps, instant_fee_bps, unbond_period_secs, Syscall::block_timestamp());
        Self { state: RefCell::new(state) }
    }

    // Keep `vft` first: `VFT_ROUTE_IDX` relies on it being service #1.
    pub fn vft(&self) -> Vft<'_> {
        Vft::new(&self.state)
    }

    pub fn pool(&self) -> Pool<'_> {
        Pool::new(&self.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(n: u64) -> ActorId { ActorId::from(n) }
    fn u(n: u128) -> U256 { U256::from(n) }
    const T0: u64 = 1_700_000_000_000;
    const ONE: u128 = 1_000_000_000_000; // 12 decimals

    fn pool(apy_bps: u32) -> PoolState {
        PoolState::new(a(1), "Vale kVARA".into(), "kVARA".into(), 12, apy_bps, 30, 7 * 86_400, T0)
    }

    #[test]
    fn first_stake_is_one_to_one_and_grows_the_reserve() {
        let mut p = pool(3_500);
        assert_eq!(p.stake(a(2), u(100 * ONE)).unwrap(), u(100 * ONE));
        assert_eq!(p.balance_of(&a(2)), u(100 * ONE));
        assert_eq!(p.reserve, u(100 * ONE));
        assert_eq!(p.total_assets(), u(100 * ONE));
    }

    #[test]
    fn rate_accrues_linearly_between_checkpoints() {
        let mut p = pool(3_500); // 35% APY
        p.stake(a(2), u(100 * ONE)).unwrap();
        let later = T0 + YEAR_SECS * 1000;
        let r = p.rate_at(later);
        assert_eq!(r, SCALE + SCALE * 35 / 100, "35% after a year");
        assert!(p.accrue(later));
        assert_eq!(p.assets_for(u(100 * ONE)).unwrap(), u(135 * ONE));
        // the second year compounds on the checkpointed rate
        assert_eq!(p.rate_at(later + YEAR_SECS * 1000), r + r * 35 / 100);
    }

    #[test]
    fn stake_after_growth_gets_fewer_shares() {
        let mut p = pool(3_500);
        p.stake(a(2), u(100 * ONE)).unwrap();
        p.accrue(T0 + YEAR_SECS * 1000);
        assert_eq!(p.stake(a(3), u(135 * ONE)).unwrap(), u(100 * ONE));
    }

    #[test]
    fn unstake_takes_fee_burns_and_draws_the_reserve() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE)).unwrap();
        let out = p.unstake(a(2), u(40 * ONE)).unwrap();
        assert_eq!(out, UnstakePreview { assets: u(40 * ONE), fee: u(40 * ONE * 30 / 10_000), net: u(40 * ONE - 40 * ONE * 30 / 10_000) });
        assert_eq!(p.balance_of(&a(2)), u(60 * ONE));
        assert_eq!(p.fees_accrued, out.fee);
        assert_eq!(p.reserve, u(100 * ONE) - out.net);
        assert_eq!(p.unstake(a(2), u(60 * ONE + 1)), Err(PoolError::InsufficientShares));
        p.revert_unstake(a(2), u(40 * ONE), &out);
        assert_eq!(p.balance_of(&a(2)), u(100 * ONE));
        assert_eq!(p.reserve, u(100 * ONE));
        assert_eq!(p.fees_accrued, U256::zero());
    }

    #[test]
    fn yield_needs_funded_rewards() {
        let mut p = pool(3_500);
        p.stake(a(2), u(100 * ONE)).unwrap();
        p.accrue(T0 + YEAR_SECS * 1000);
        // 135 VARA owed, only 100 in the reserve: refused without side effects.
        let err = p.unstake(a(2), u(100 * ONE)).unwrap_err();
        assert_eq!(err, PoolError::InsufficientReserve { available: u(100 * ONE) });
        assert_eq!(p.balance_of(&a(2)), u(100 * ONE));
        p.fund(u(50 * ONE)).unwrap();
        let out = p.unstake(a(2), u(100 * ONE)).unwrap();
        assert_eq!(out.assets, u(135 * ONE));
        assert_eq!(p.reserve, u(150 * ONE) - out.net);
        assert_eq!(p.fund(U256::zero()), Err(PoolError::ZeroAmount));
    }

    #[test]
    fn unbond_locks_assets_and_claims_from_the_reserve_after_the_period() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE)).unwrap();
        let e = p.request_unbond(a(2), u(50 * ONE), T0).unwrap();
        assert_eq!(e.assets, u(50 * ONE));
        assert_eq!(e.claimable_at, T0 + 7 * 86_400 * 1000);
        assert_eq!(p.unbonding_total, u(50 * ONE));
        assert_eq!(p.balance_of(&a(2)), u(50 * ONE));
        assert_eq!(p.take_unbond(a(2), e.id, T0 + 1), Err(PoolError::UnbondNotReady { claimable_at: e.claimable_at }));
        assert_eq!(p.take_unbond(a(3), e.id, e.claimable_at), Err(PoolError::UnbondNotFound));
        let taken = p.take_unbond(a(2), e.id, e.claimable_at).unwrap();
        assert_eq!(taken, e);
        assert_eq!(p.reserve, u(50 * ONE));
        assert!(p.unbonds_of(&a(2)).is_empty());
        assert_eq!(p.unbonding_total, U256::zero());
        p.restore_unbond(a(2), taken.clone());
        assert_eq!(p.unbonds_of(&a(2)), vec![taken]);
        assert_eq!(p.reserve, u(100 * ONE));
        assert_eq!(p.unbonding_total, u(50 * ONE));
    }

    #[test]
    fn minimum_stake_and_pause() {
        let mut p = pool(0);
        assert_eq!(p.stake(a(2), u(MIN_STAKE - 1)), Err(PoolError::BelowMinimum { min: u(MIN_STAKE) }));
        assert_eq!(p.stake(a(2), U256::zero()), Err(PoolError::ZeroAmount));
        p.paused = true;
        assert_eq!(p.stake(a(2), u(ONE)), Err(PoolError::Paused));
        assert_eq!(p.request_unbond(a(2), u(1), T0), Err(PoolError::Paused));
    }

    #[test]
    fn receipt_token_transfers_and_allowances() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE)).unwrap();
        p.transfer_shares(a(2), a(3), u(25 * ONE)).unwrap();
        assert_eq!(p.balance_of(&a(3)), u(25 * ONE));
        p.approve_shares(a(3), a(4), u(10 * ONE)).unwrap();
        p.spend_allowance(a(3), a(4), u(6 * ONE)).unwrap();
        assert_eq!(p.allowance(&a(3), &a(4)), u(4 * ONE));
        assert_eq!(p.spend_allowance(a(3), a(4), u(4 * ONE + 1)), Err(PoolError::InsufficientAllowance));
        assert_eq!(p.total_shares, u(100 * ONE), "transfers do not change supply");
    }

    #[test]
    fn sessions_act_for_their_owner_until_expiry() {
        let mut p = pool(0);
        let all = vec![SessionAction::Stake, SessionAction::Unstake, SessionAction::Unbond, SessionAction::Claim];
        assert_eq!(p.actor_for(a(2), SessionAction::Stake, T0).unwrap(), a(2));
        assert_eq!(p.create_session(a(2), a(2), 60, all.clone(), T0), Err(PoolError::BadSession));
        assert_eq!(p.create_session(a(2), a(9), MAX_SESSION_SECS + 1, all.clone(), T0), Err(PoolError::BadSession));
        let s = p.create_session(a(2), a(9), 60, vec![SessionAction::Unstake], T0).unwrap();
        assert_eq!(s.expires_at, T0 + 60_000);
        assert_eq!(p.actor_for(a(9), SessionAction::Unstake, T0 + 1).unwrap(), a(2));
        assert_eq!(p.actor_for(a(9), SessionAction::Claim, T0 + 1), Err(PoolError::SessionNotAllowed));
        assert_eq!(p.actor_for(a(9), SessionAction::Unstake, T0 + 60_000), Err(PoolError::SessionExpired));
        assert_eq!(p.create_session(a(3), a(9), 60, all.clone(), T0), Err(PoolError::BadSession));
        assert!(p.revoke_session(a(2)).is_some());
        assert_eq!(p.actor_for(a(9), SessionAction::Unstake, T0 + 1).unwrap(), a(9));
    }

    #[test]
    fn info_projects_rate() {
        let mut p = pool(3_500);
        p.stake(a(2), u(100 * ONE)).unwrap();
        let i = p.info(T0 + YEAR_SECS * 1000);
        assert_eq!(i.rate, SCALE + SCALE * 35 / 100);
        assert_eq!(i.total_assets, u(135 * ONE));
        assert_eq!(i.reserve, u(100 * ONE));
        assert_eq!(i.min_stake, u(MIN_STAKE));
    }
}
