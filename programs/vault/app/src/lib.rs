//! Vale Protocol vault: a liquid staking style vault for one underlying VFT.
//!
//! The vault program *is* the receipt token (kUSDC, kUSDT): its `Vft` service is
//! the standard fungible token surface over share balances, and its `Vault`
//! service moves the underlying in and out.
//!
//! Accounting is asset based (ERC-4626 shape) and nothing is promised that the vault does
//! not hold. The rate (assets per share, scaled by `1e18`) is `distributable / total_shares`,
//! where `distributable` is what the vault holds minus collected fees, minus assets locked for
//! unbonds, minus rewards still vesting. Rewards arrive through `fund_rewards` and vest
//! linearly over the vesting period, so a holder earns exactly the slice of every reward that
//! vested while they held shares: entering and leaving in the same block earns nothing.
//!
//! Async rules (Gear persists state at every `.await`):
//! 1. validate before the first await, never hold a borrow across it;
//! 2. burn shares and take the payout out of `holdings` *before* the outbound transfer,
//!    and compensate on failure, returning `Err` instead of panicking after an await;
//! 3. inbound tokens are counted only after the transfer succeeded, so in-flight deposits
//!    are invisible to the rate until they are backed.

#![no_std]

use core::cell::RefCell;
use sails_rs::{collections::BTreeMap, prelude::*};

#[allow(dead_code, unused_imports)]
pub mod token_client;
use sails_rs::client::Program as _;
use token_client::{DemoToken as _, DemoTokenProgram, vft::Vft as _};

/// Route index of the `vft` service inside the program (first service => 1).
const VFT_ROUTE_IDX: u8 = 1;
/// Rate scale: 1e18.
pub const SCALE: U256 = U256([1_000_000_000_000_000_000u64, 0, 0, 0]);
pub const BPS: u64 = 10_000;
pub const YEAR_SECS: u64 = 31_536_000;
/// Smallest deposit in base units so share rounding can never produce zero.
pub const MIN_DEPOSIT: u64 = 1_000;
pub const MAX_FEE_BPS: u32 = 1_000; // 10%
/// Rewards vest over at least an hour (so one block releases a negligible slice) and at most a year.
pub const MIN_VESTING_SECS: u64 = 3_600;
pub const MAX_VESTING_SECS: u64 = 365 * 86_400;
/// Longest session a user may register.
pub const MAX_SESSION_SECS: u64 = 30 * 86_400;
/// Gas handed to each call into the underlying token (measured ~1.5B on a 1.10 node).
pub const TOKEN_CALL_GAS: u64 = 6_000_000_000;
/// Gas a caller must still have when an async command starts, so the segment that
/// runs after the token reply can never run out of gas with funds already moved.
/// One token call: instantiate again (~17B on a 1.10 node) + call + margin.
pub const GAS_ONE_CALL: u64 = 40_000_000_000;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    Paused,
    ZeroAddress,
    ZeroAmount,
    BelowMinimum { min: U256 },
    InsufficientShares,
    InsufficientBalance,
    InsufficientAllowance,
    /// The vault's distributable assets cannot cover this payout; nothing changed.
    InsufficientReserve { available: U256 },
    Unauthorized,
    Overflow,
    BadConfig,
    UnbondNotFound,
    UnbondNotReady { claimable_at: u64 },
    /// The underlying token rejected the transfer (allowance, balance, paused).
    TokenRejected(String),
    /// The cross-program call failed to complete (timeout, transport, panic).
    TokenCall(String),
    /// The message carried less gas than the async flow needs; nothing was moved.
    NotEnoughGas { required: u64 },
    /// The session key acting for the owner has expired.
    SessionExpired,
    /// The session key acting for the owner may not perform this action.
    SessionNotAllowed,
    /// Session key, duration or action list is invalid, or no matching proposal exists.
    BadSession,
}

/// Actions a session key may perform for its owner.
#[sails_rs::sails_type]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Deposit,
    Redeem,
    Unbond,
    Claim,
}

/// A session key registered by an owner: messages signed by `key` act for the owner until
/// `expires_at`. Payouts always go to the owner, never to the key. A session is proposed by
/// the owner and only takes effect once the key itself accepts it, so nobody can bind an
/// address they do not control.
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
pub struct RedeemPreview {
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
pub struct VaultInfo {
    pub underlying: ActorId,
    pub admin: ActorId,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    /// Assets per share scaled by 1e18: distributable assets over shares, now.
    pub rate: U256,
    /// Annualised release rate of the rewards currently vesting, over distributable assets.
    /// Zero once the current tranche has fully vested.
    pub apy_bps: u32,
    pub instant_fee_bps: u32,
    pub unbond_period_secs: u64,
    pub vesting_period_secs: u64,
    pub total_shares: U256,
    /// Assets owed to share holders: the distributable amount.
    pub total_assets: U256,
    /// Underlying the vault physically holds (deposits, rewards, fees, unbonds).
    pub holdings: U256,
    /// Holdings minus fees, unbonds and rewards still vesting.
    pub distributable: U256,
    /// Rewards not yet released.
    pub locked_rewards: U256,
    /// When the current tranche is fully vested (0 when nothing is vesting).
    pub vesting_ends_at: u64,
    pub fees_accrued: U256,
    pub unbonding_total: U256,
    pub min_deposit: U256,
    pub paused: bool,
}

// ---------------------------------------------------------------------------
// State and pure transitions (unit tested on the host)
// ---------------------------------------------------------------------------

pub struct VaultState {
    pub admin: ActorId,
    pub underlying: ActorId,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_shares: U256,
    pub balances: BTreeMap<ActorId, U256>,
    pub allowances: BTreeMap<(ActorId, ActorId), U256>,
    /// Rate used while the vault holds no shares: the constructor's initial rate, then the
    /// last rate seen before the vault emptied, so the receipt keeps its price across an empty spell.
    pub base_rate: U256,
    pub instant_fee_bps: u32,
    pub unbond_period_ms: u64,
    pub vesting_period_ms: u64,
    /// Underlying the vault holds and has accounted for. Internal: a plain token transfer to the
    /// vault is invisible here, so nothing outside `deposit` and `fund` moves the rate.
    pub holdings: U256,
    pub fees_accrued: U256,
    pub unbonding_total: U256,
    /// The rewards tranche currently vesting: `vest_amount` is released linearly between
    /// `vest_start_ms` and `vest_end_ms`.
    pub vest_amount: U256,
    pub vest_start_ms: u64,
    pub vest_end_ms: u64,
    pub paused: bool,
    pub unbonds: BTreeMap<ActorId, Vec<Unbond>>,
    pub next_unbond_id: u64,
    /// owner -> active session
    pub sessions: BTreeMap<ActorId, Session>,
    /// session key -> owner (active sessions only)
    pub session_keys: BTreeMap<ActorId, ActorId>,
    /// owner -> proposed session, waiting for the key to accept
    pub pending_sessions: BTreeMap<ActorId, Session>,
}

impl VaultState {
    pub fn new(admin: ActorId, underlying: ActorId, name: String, symbol: String, decimals: u8, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64, initial_rate: U256) -> Self {
        Self {
            admin,
            underlying,
            name,
            symbol,
            decimals,
            total_shares: U256::zero(),
            balances: BTreeMap::new(),
            allowances: BTreeMap::new(),
            // A pool that starts at a rate above 1.0 continues where an earlier pool left off.
            base_rate: if initial_rate < SCALE { SCALE } else { initial_rate },
            instant_fee_bps,
            unbond_period_ms: unbond_period_secs.saturating_mul(1000),
            vesting_period_ms: vesting_period_secs.clamp(MIN_VESTING_SECS, MAX_VESTING_SECS).saturating_mul(1000),
            holdings: U256::zero(),
            fees_accrued: U256::zero(),
            unbonding_total: U256::zero(),
            vest_amount: U256::zero(),
            vest_start_ms: 0,
            vest_end_ms: 0,
            paused: false,
            unbonds: BTreeMap::new(),
            next_unbond_id: 1,
            sessions: BTreeMap::new(),
            session_keys: BTreeMap::new(),
            pending_sessions: BTreeMap::new(),
        }
    }

    // --- rate ---------------------------------------------------------------

    /// Rewards of the current tranche not yet released at `now_ms`.
    pub fn locked_rewards(&self, now_ms: u64) -> U256 {
        if self.vest_amount.is_zero() || now_ms >= self.vest_end_ms { return U256::zero(); }
        if now_ms <= self.vest_start_ms { return self.vest_amount; }
        let span = U256::from(self.vest_end_ms - self.vest_start_ms);
        let left = U256::from(self.vest_end_ms - now_ms);
        self.vest_amount.saturating_mul(left) / span
    }

    /// Assets that belong to share holders right now.
    pub fn distributable(&self, now_ms: u64) -> U256 {
        self.holdings.saturating_sub(self.fees_accrued).saturating_sub(self.unbonding_total).saturating_sub(self.locked_rewards(now_ms))
    }

    /// True while the vault has no shares (or nothing distributable) and prices at `base_rate`.
    fn empty(&self, now_ms: u64) -> bool {
        self.total_shares.is_zero() || self.distributable(now_ms).is_zero()
    }

    /// Assets per share scaled by 1e18 at `now_ms`.
    pub fn rate_at(&self, now_ms: u64) -> U256 {
        if self.empty(now_ms) { return self.base_rate; }
        self.distributable(now_ms).saturating_mul(SCALE) / self.total_shares
    }

    /// Shares minted for `assets` at `now_ms` (rounded down, in the vault's favour).
    pub fn shares_for(&self, assets: U256, now_ms: u64) -> Result<U256, VaultError> {
        if self.empty(now_ms) {
            return assets.checked_mul(SCALE).ok_or(VaultError::Overflow).map(|v| v / self.base_rate);
        }
        assets.checked_mul(self.total_shares).ok_or(VaultError::Overflow).map(|v| v / self.distributable(now_ms))
    }

    /// Assets owed for `shares` at `now_ms` (rounded down, in the vault's favour).
    pub fn assets_for(&self, shares: U256, now_ms: u64) -> Result<U256, VaultError> {
        if self.empty(now_ms) {
            return shares.checked_mul(self.base_rate).ok_or(VaultError::Overflow).map(|v| v / SCALE);
        }
        shares.checked_mul(self.distributable(now_ms)).ok_or(VaultError::Overflow).map(|v| v / self.total_shares)
    }

    pub fn total_assets(&self, now_ms: u64) -> U256 {
        if self.total_shares.is_zero() { U256::zero() } else { self.distributable(now_ms) }
    }

    /// Annualised release rate of the vesting tranche over distributable assets, in bps.
    pub fn apy_bps(&self, now_ms: u64) -> u32 {
        let d = self.distributable(now_ms);
        if self.vest_amount.is_zero() || now_ms >= self.vest_end_ms || d.is_zero() || self.total_shares.is_zero() { return 0; }
        let span = U256::from(self.vest_end_ms - self.vest_start_ms);
        let bps = self.vest_amount.saturating_mul(U256::from(YEAR_SECS * 1000)).saturating_mul(U256::from(BPS)) / (span.saturating_mul(d));
        if bps > U256::from(u32::MAX) { u32::MAX } else { bps.low_u32() }
    }

    pub fn preview_redeem(&self, shares: U256, now_ms: u64) -> Result<RedeemPreview, VaultError> {
        let assets = self.assets_for(shares, now_ms)?;
        let fee = assets.saturating_mul(U256::from(self.instant_fee_bps)) / U256::from(BPS);
        Ok(RedeemPreview { assets, fee, net: assets - fee })
    }

    // --- shares (receipt token) ---------------------------------------------

    pub fn balance_of(&self, who: &ActorId) -> U256 {
        self.balances.get(who).copied().unwrap_or_default()
    }

    pub fn allowance(&self, owner: &ActorId, spender: &ActorId) -> U256 {
        self.allowances.get(&(*owner, *spender)).copied().unwrap_or_default()
    }

    fn ensure_live(&self) -> Result<(), VaultError> {
        if self.paused { Err(VaultError::Paused) } else { Ok(()) }
    }

    fn credit(&mut self, to: ActorId, value: U256) -> Result<(), VaultError> {
        let new = self.balance_of(&to).checked_add(value).ok_or(VaultError::Overflow)?;
        self.balances.insert(to, new);
        Ok(())
    }

    fn debit(&mut self, from: ActorId, value: U256) -> Result<(), VaultError> {
        let new = self.balance_of(&from).checked_sub(value).ok_or(VaultError::InsufficientBalance)?;
        if new.is_zero() { self.balances.remove(&from); } else { self.balances.insert(from, new); }
        Ok(())
    }

    pub fn transfer_shares(&mut self, from: ActorId, to: ActorId, value: U256) -> Result<(), VaultError> {
        self.ensure_live()?;
        if to.is_zero() { return Err(VaultError::ZeroAddress); }
        if value.is_zero() { return Err(VaultError::ZeroAmount); }
        self.debit(from, value)?;
        self.credit(to, value)
    }

    pub fn approve_shares(&mut self, owner: ActorId, spender: ActorId, value: U256) -> Result<(), VaultError> {
        self.ensure_live()?;
        if spender.is_zero() { return Err(VaultError::ZeroAddress); }
        if value.is_zero() { self.allowances.remove(&(owner, spender)); } else { self.allowances.insert((owner, spender), value); }
        Ok(())
    }

    pub fn spend_allowance(&mut self, owner: ActorId, spender: ActorId, value: U256) -> Result<(), VaultError> {
        let current = self.allowance(&owner, &spender);
        if current == U256::MAX { return Ok(()); }
        let rest = current.checked_sub(value).ok_or(VaultError::InsufficientAllowance)?;
        if rest.is_zero() { self.allowances.remove(&(owner, spender)); } else { self.allowances.insert((owner, spender), rest); }
        Ok(())
    }

    pub fn mint_shares(&mut self, to: ActorId, value: U256) -> Result<(), VaultError> {
        let supply = self.total_shares.checked_add(value).ok_or(VaultError::Overflow)?;
        self.credit(to, value)?;
        self.total_shares = supply;
        Ok(())
    }

    /// Burn shares. When the last share goes, the vault remembers its rate for the empty spell.
    pub fn burn_shares(&mut self, from: ActorId, value: U256, now_ms: u64) -> Result<(), VaultError> {
        if value.is_zero() { return Err(VaultError::ZeroAmount); }
        let rate_before = self.rate_at(now_ms);
        self.debit(from, value).map_err(|_| VaultError::InsufficientShares)?;
        self.total_shares = self.total_shares.checked_sub(value).ok_or(VaultError::Overflow)?;
        if self.total_shares.is_zero() { self.base_rate = rate_before; }
        Ok(())
    }

    // --- vault flows --------------------------------------------------------

    /// Assets that belong to nobody (dust and rewards vested while the vault was empty) go to
    /// the fee bucket, so the first depositor after an empty spell cannot claim them.
    fn sweep_orphans(&mut self, now_ms: u64) {
        if self.total_shares.is_zero() {
            let orphaned = self.distributable(now_ms);
            self.fees_accrued = self.fees_accrued.saturating_add(orphaned);
        }
    }

    /// Pre-await checks for a deposit. Returns the shares the deposit would mint now.
    pub fn check_deposit(&self, assets: U256, now_ms: u64) -> Result<U256, VaultError> {
        self.ensure_live()?;
        if assets.is_zero() { return Err(VaultError::ZeroAmount); }
        if assets < U256::from(MIN_DEPOSIT) { return Err(VaultError::BelowMinimum { min: U256::from(MIN_DEPOSIT) }); }
        let shares = self.shares_for(assets, now_ms)?;
        if shares.is_zero() { return Err(VaultError::BelowMinimum { min: U256::from(MIN_DEPOSIT) }); }
        Ok(shares)
    }

    /// Post-await settlement of a deposit whose tokens have arrived.
    pub fn settle_deposit(&mut self, owner: ActorId, assets: U256, now_ms: u64) -> Result<U256, VaultError> {
        self.sweep_orphans(now_ms);
        let shares = self.shares_for(assets, now_ms)?.max(U256::one());
        self.mint_shares(owner, shares)?;
        self.holdings = self.holdings.checked_add(assets).ok_or(VaultError::Overflow)?;
        Ok(shares)
    }

    /// Burn shares for an instant exit and take the payout out of holdings (pre-await), so
    /// nothing between here and the transfer can price off assets that are already promised.
    pub fn begin_redeem(&mut self, owner: ActorId, shares: U256, now_ms: u64) -> Result<RedeemPreview, VaultError> {
        self.ensure_live()?;
        if shares.is_zero() { return Err(VaultError::ZeroAmount); }
        if self.balance_of(&owner) < shares { return Err(VaultError::InsufficientShares); }
        let p = self.preview_redeem(shares, now_ms)?;
        if p.net.is_zero() { return Err(VaultError::ZeroAmount); }
        let available = self.distributable(now_ms);
        if p.assets > available { return Err(VaultError::InsufficientReserve { available }); }
        self.burn_shares(owner, shares, now_ms)?;
        self.fees_accrued = self.fees_accrued.saturating_add(p.fee);
        self.holdings -= p.net;
        Ok(p)
    }

    /// Undo `begin_redeem` after a failed payout.
    pub fn revert_redeem(&mut self, owner: ActorId, shares: U256, p: &RedeemPreview) {
        let _ = self.mint_shares(owner, shares);
        self.fees_accrued = self.fees_accrued.saturating_sub(p.fee);
        self.holdings = self.holdings.saturating_add(p.net);
    }

    /// Count rewards whose tokens have arrived: they vest linearly. What is still locked from
    /// the previous tranche is folded in, and the new tranche's length is the amount-weighted
    /// average of the time left and a full vesting period (Yearn v3 style): dust cannot
    /// stretch a running tranche, and a new tranche after a finished one gets a full period.
    pub fn fund(&mut self, assets: U256, now_ms: u64) -> Result<(), VaultError> {
        if assets.is_zero() { return Err(VaultError::ZeroAmount); }
        let remaining = self.locked_rewards(now_ms);
        let period = U256::from(self.vesting_period_ms);
        let total = remaining.checked_add(assets).ok_or(VaultError::Overflow)?;
        let time_left = if remaining.is_zero() {
            period
        } else {
            let left = U256::from(self.vest_end_ms.saturating_sub(now_ms));
            (remaining.saturating_mul(left).saturating_add(assets.saturating_mul(period))) / total
        };
        let time_left = time_left.max(U256::one()).low_u64();
        self.vest_amount = total;
        self.vest_start_ms = now_ms;
        self.vest_end_ms = now_ms.saturating_add(time_left);
        self.holdings = self.holdings.checked_add(assets).ok_or(VaultError::Overflow)?;
        Ok(())
    }

    pub fn request_unbond(&mut self, owner: ActorId, shares: U256, now_ms: u64) -> Result<Unbond, VaultError> {
        self.ensure_live()?;
        if shares.is_zero() { return Err(VaultError::ZeroAmount); }
        if self.balance_of(&owner) < shares { return Err(VaultError::InsufficientShares); }
        let assets = self.assets_for(shares, now_ms)?;
        if assets.is_zero() { return Err(VaultError::ZeroAmount); }
        let available = self.distributable(now_ms);
        if assets > available { return Err(VaultError::InsufficientReserve { available }); }
        self.burn_shares(owner, shares, now_ms)?;
        self.unbonding_total = self.unbonding_total.checked_add(assets).ok_or(VaultError::Overflow)?;
        let entry = Unbond { id: self.next_unbond_id, shares, assets, requested_at: now_ms, claimable_at: now_ms.saturating_add(self.unbond_period_ms) };
        self.next_unbond_id += 1;
        self.unbonds.entry(owner).or_default().push(entry.clone());
        Ok(entry)
    }

    /// Remove a claimable unbond entry and take its assets out of holdings (pre-await).
    pub fn take_unbond(&mut self, owner: ActorId, id: u64, now_ms: u64) -> Result<Unbond, VaultError> {
        self.ensure_live()?;
        let list = self.unbonds.get(&owner).ok_or(VaultError::UnbondNotFound)?;
        let idx = list.iter().position(|u| u.id == id).ok_or(VaultError::UnbondNotFound)?;
        if now_ms < list[idx].claimable_at { return Err(VaultError::UnbondNotReady { claimable_at: list[idx].claimable_at }); }
        if list[idx].assets > self.holdings { return Err(VaultError::InsufficientReserve { available: self.holdings }); }
        let list = self.unbonds.get_mut(&owner).ok_or(VaultError::UnbondNotFound)?;
        let entry = list.remove(idx);
        if list.is_empty() { self.unbonds.remove(&owner); }
        self.unbonding_total = self.unbonding_total.saturating_sub(entry.assets);
        self.holdings -= entry.assets;
        Ok(entry)
    }

    /// Put an unbond entry back after a failed payout.
    pub fn restore_unbond(&mut self, owner: ActorId, entry: Unbond) {
        self.unbonding_total = self.unbonding_total.saturating_add(entry.assets);
        self.holdings = self.holdings.saturating_add(entry.assets);
        let list = self.unbonds.entry(owner).or_default();
        list.push(entry);
        list.sort_by_key(|u| u.id);
    }

    /// Take the accrued fees out of holdings (pre-await). Returns the amount to pay.
    pub fn take_fees(&mut self) -> Result<U256, VaultError> {
        let amount = self.fees_accrued;
        if amount.is_zero() { return Err(VaultError::ZeroAmount); }
        if amount > self.holdings { return Err(VaultError::InsufficientReserve { available: self.holdings }); }
        self.fees_accrued = U256::zero();
        self.holdings -= amount;
        Ok(amount)
    }

    pub fn restore_fees(&mut self, amount: U256) {
        self.fees_accrued = self.fees_accrued.saturating_add(amount);
        self.holdings = self.holdings.saturating_add(amount);
    }

    pub fn unbonds_of(&self, owner: &ActorId) -> Vec<Unbond> {
        self.unbonds.get(owner).cloned().unwrap_or_default()
    }

    fn ensure_admin(&self, who: ActorId) -> Result<(), VaultError> {
        if self.admin == who { Ok(()) } else { Err(VaultError::Unauthorized) }
    }

    pub fn set_config(&mut self, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64) -> Result<(), VaultError> {
        if instant_fee_bps > MAX_FEE_BPS || !(MIN_VESTING_SECS..=MAX_VESTING_SECS).contains(&vesting_period_secs) { return Err(VaultError::BadConfig); }
        self.instant_fee_bps = instant_fee_bps;
        self.unbond_period_ms = unbond_period_secs.saturating_mul(1000);
        self.vesting_period_ms = vesting_period_secs.saturating_mul(1000);
        Ok(())
    }

    // --- sessions ------------------------------------------------------------

    /// Who a message acts for: the sender itself, or the owner of the accepted session key it holds.
    pub fn actor_for(&self, sender: ActorId, action: SessionAction, now_ms: u64) -> Result<ActorId, VaultError> {
        let Some(owner) = self.session_keys.get(&sender) else { return Ok(sender) };
        let session = self.sessions.get(owner).ok_or(VaultError::SessionExpired)?;
        if now_ms >= session.expires_at { return Err(VaultError::SessionExpired); }
        if !session.actions.contains(&action) { return Err(VaultError::SessionNotAllowed); }
        Ok(*owner)
    }

    /// Propose a session. Nothing changes for `key` until it accepts.
    pub fn propose_session(&mut self, owner: ActorId, key: ActorId, duration_secs: u64, actions: Vec<SessionAction>, now_ms: u64) -> Result<Session, VaultError> {
        if key.is_zero() || key == owner || duration_secs == 0 || duration_secs > MAX_SESSION_SECS || actions.is_empty() { return Err(VaultError::BadSession); }
        // A key may serve one owner.
        if let Some(other) = self.session_keys.get(&key) { if *other != owner { return Err(VaultError::BadSession); } }
        let session = Session { key, expires_at: now_ms.saturating_add(duration_secs.saturating_mul(1000)), actions };
        self.pending_sessions.insert(owner, session.clone());
        Ok(session)
    }

    /// The key accepts the session `owner` proposed for it; from now on it acts for `owner`.
    /// Replaces any session the owner had before.
    pub fn accept_session(&mut self, key: ActorId, owner: ActorId, now_ms: u64) -> Result<Session, VaultError> {
        let proposed = self.pending_sessions.get(&owner).filter(|s| s.key == key).ok_or(VaultError::BadSession)?;
        if now_ms >= proposed.expires_at { return Err(VaultError::SessionExpired); }
        if let Some(other) = self.session_keys.get(&key) { if *other != owner { return Err(VaultError::BadSession); } }
        let session = self.pending_sessions.remove(&owner).ok_or(VaultError::BadSession)?;
        if let Some(old) = self.sessions.remove(&owner) { self.session_keys.remove(&old.key); }
        self.sessions.insert(owner, session.clone());
        self.session_keys.insert(key, owner);
        Ok(session)
    }

    pub fn revoke_session(&mut self, owner: ActorId) -> bool {
        let pending = self.pending_sessions.remove(&owner).is_some();
        let Some(old) = self.sessions.remove(&owner) else { return pending };
        self.session_keys.remove(&old.key);
        true
    }

    pub fn info(&self, now_ms: u64) -> VaultInfo {
        let locked = self.locked_rewards(now_ms);
        VaultInfo {
            underlying: self.underlying,
            admin: self.admin,
            name: self.name.clone(),
            symbol: self.symbol.clone(),
            decimals: self.decimals,
            rate: self.rate_at(now_ms),
            apy_bps: self.apy_bps(now_ms),
            instant_fee_bps: self.instant_fee_bps,
            unbond_period_secs: self.unbond_period_ms / 1000,
            vesting_period_secs: self.vesting_period_ms / 1000,
            total_shares: self.total_shares,
            total_assets: self.total_assets(now_ms),
            holdings: self.holdings,
            distributable: self.distributable(now_ms),
            locked_rewards: locked,
            vesting_ends_at: if locked.is_zero() { 0 } else { self.vest_end_ms },
            fees_accrued: self.fees_accrued,
            unbonding_total: self.unbonding_total,
            min_deposit: U256::from(MIN_DEPOSIT),
            paused: self.paused,
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
    state: &'a RefCell<VaultState>,
}

impl<'a> Vft<'a> {
    pub fn new(state: &'a RefCell<VaultState>) -> Self {
        Self { state }
    }
}

#[sails_rs::service(events = VftEvent)]
impl Vft<'_> {
    #[export(unwrap_result)]
    pub fn approve(&mut self, spender: ActorId, value: U256) -> Result<bool, VaultError> {
        let owner = Syscall::message_source();
        self.state.borrow_mut().approve_shares(owner, spender, value)?;
        self.emit_event(VftEvent::Approval { owner, spender, value }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn transfer(&mut self, to: ActorId, value: U256) -> Result<bool, VaultError> {
        let from = Syscall::message_source();
        self.state.borrow_mut().transfer_shares(from, to, value)?;
        self.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn transfer_from(&mut self, from: ActorId, to: ActorId, value: U256) -> Result<bool, VaultError> {
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

fn emit_vft_transfer(state: &RefCell<VaultState>, from: ActorId, to: ActorId, value: U256) {
    use sails_rs::gstd::services::Service as _;
    let vft = Vft::new(state).expose(VFT_ROUTE_IDX);
    vft.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
}

// ---------------------------------------------------------------------------
// Vault service
// ---------------------------------------------------------------------------

#[sails_rs::sails_type]
#[sails_rs::event]
#[derive(Debug, Clone, PartialEq, Eq)]
// Alphabetical: the IDL sorts events, the interface hash uses declaration order.
pub enum VaultEvent {
    AdminTransferred { from: ActorId, to: ActorId },
    Claimed { owner: ActorId, id: u64, assets: U256 },
    ConfigChanged { instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64 },
    Deposited { owner: ActorId, assets: U256, shares: U256, rate: U256 },
    FeesCollected { to: ActorId, assets: U256 },
    Paused { by: ActorId },
    Redeemed { owner: ActorId, shares: U256, assets: U256, fee: U256, rate: U256 },
    Resumed { by: ActorId },
    RewardsFunded { from: ActorId, assets: U256, holdings: U256, vesting_ends_at: u64 },
    SessionAccepted { owner: ActorId, key: ActorId, expires_at: u64 },
    SessionProposed { owner: ActorId, key: ActorId, expires_at: u64 },
    SessionRevoked { owner: ActorId },
    UnbondRequested { owner: ActorId, id: u64, shares: U256, assets: U256, claimable_at: u64 },
}

pub struct Vault<'a> {
    state: &'a RefCell<VaultState>,
}

impl<'a> Vault<'a> {
    pub fn new(state: &'a RefCell<VaultState>) -> Self {
        Self { state }
    }
}

fn now_ms() -> u64 {
    Syscall::block_timestamp()
}

/// Refuse to start an async flow unless enough gas remains for every segment of it.
fn ensure_gas(required: u64) -> Result<(), VaultError> {
    if Syscall::gas_available() < required { Err(VaultError::NotEnoughGas { required }) } else { Ok(()) }
}

/// Pull `assets` of the underlying from `from` into the vault.
async fn pull_underlying(underlying: ActorId, from: ActorId, assets: U256) -> Result<(), VaultError> {
    let me = Syscall::program_id();
    match DemoTokenProgram::client(underlying).vft().transfer_from(from, me, assets).with_gas_limit(TOKEN_CALL_GAS).await {
        Ok(Ok(true)) => Ok(()),
        Ok(Ok(false)) => Err(VaultError::TokenRejected("transfer returned false".into())),
        Ok(Err(e)) => Err(VaultError::TokenRejected(format!("{e:?}"))),
        Err(e) => Err(VaultError::TokenCall(format!("{e:?}"))),
    }
}

/// Push `assets` of the underlying from the vault to `to`.
async fn push_underlying(underlying: ActorId, to: ActorId, assets: U256) -> Result<(), VaultError> {
    match DemoTokenProgram::client(underlying).vft().transfer(to, assets).with_gas_limit(TOKEN_CALL_GAS).await {
        Ok(Ok(true)) => Ok(()),
        Ok(Ok(false)) => Err(VaultError::TokenRejected("transfer returned false".into())),
        Ok(Err(e)) => Err(VaultError::TokenRejected(format!("{e:?}"))),
        Err(e) => Err(VaultError::TokenCall(format!("{e:?}"))),
    }
}

#[sails_rs::service(events = VaultEvent)]
impl Vault<'_> {
    // Deposit `assets` of the underlying (requires prior approval). Returns shares minted.
    #[export]
    pub async fn deposit(&mut self, assets: U256) -> Result<U256, VaultError> {
        ensure_gas(GAS_ONE_CALL)?;
        let (owner, underlying) = {
            let s = self.state.borrow();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Deposit, now)?;
            s.check_deposit(assets, now)?;
            (owner, s.underlying)
        };
        pull_underlying(underlying, owner, assets).await?;
        let (shares, rate) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let shares = s.settle_deposit(owner, assets, now)?;
            (shares, s.rate_at(now))
        };
        emit_vft_transfer(self.state, ActorId::zero(), owner, shares);
        self.emit_event(VaultEvent::Deposited { owner, assets, shares, rate }).expect("event");
        Ok(shares)
    }

    // Instant exit: burn `shares`, receive assets at the current rate minus the instant fee.
    #[export]
    pub async fn redeem(&mut self, shares: U256) -> Result<RedeemPreview, VaultError> {
        ensure_gas(GAS_ONE_CALL)?;
        let (owner, underlying, preview, rate) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Redeem, now)?;
            let rate = s.rate_at(now);
            let p = s.begin_redeem(owner, shares, now)?;
            (owner, s.underlying, p, rate)
        };
        emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
        match push_underlying(underlying, owner, preview.net).await {
            Ok(()) => {
                self.emit_event(VaultEvent::Redeemed { owner, shares, assets: preview.assets, fee: preview.fee, rate }).expect("event");
                Ok(preview)
            }
            Err(e) => {
                self.state.borrow_mut().revert_redeem(owner, shares, &preview);
                emit_vft_transfer(self.state, ActorId::zero(), owner, shares);
                Err(e)
            }
        }
    }

    // Timed exit: burn `shares` now, lock assets at the current rate, claim after the unbond period.
    #[export]
    pub fn request_unbond(&mut self, shares: U256) -> Result<Unbond, VaultError> {
        let (owner, entry) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Unbond, now)?;
            (owner, s.request_unbond(owner, shares, now)?)
        };
        emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
        self.emit_event(VaultEvent::UnbondRequested { owner, id: entry.id, shares, assets: entry.assets, claimable_at: entry.claimable_at }).expect("event");
        Ok(entry)
    }

    // Claim a matured unbond entry. Returns the assets paid.
    #[export]
    pub async fn claim(&mut self, id: u64) -> Result<U256, VaultError> {
        ensure_gas(GAS_ONE_CALL)?;
        let (owner, underlying, entry) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Claim, now)?;
            let e = s.take_unbond(owner, id, now)?;
            (owner, s.underlying, e)
        };
        match push_underlying(underlying, owner, entry.assets).await {
            Ok(()) => {
                self.emit_event(VaultEvent::Claimed { owner, id, assets: entry.assets }).expect("event");
                Ok(entry.assets)
            }
            Err(e) => {
                self.state.borrow_mut().restore_unbond(owner, entry);
                Err(e)
            }
        }
    }

    // Pull `assets` of the underlying from the caller (requires prior approval) into the
    // rewards tranche; they vest linearly over the vesting period. Anyone may fund rewards.
    // Returns the vault's holdings after funding.
    #[export]
    pub async fn fund_rewards(&mut self, assets: U256) -> Result<U256, VaultError> {
        ensure_gas(GAS_ONE_CALL)?;
        let from = Syscall::message_source();
        let underlying = {
            let s = self.state.borrow();
            if assets.is_zero() { return Err(VaultError::ZeroAmount); }
            s.underlying
        };
        pull_underlying(underlying, from, assets).await?;
        let (holdings, vesting_ends_at) = {
            let mut s = self.state.borrow_mut();
            s.fund(assets, now_ms())?;
            (s.holdings, s.vest_end_ms)
        };
        self.emit_event(VaultEvent::RewardsFunded { from, assets, holdings, vesting_ends_at }).expect("event");
        Ok(holdings)
    }

    // --- admin ---------------------------------------------------------------

    #[export]
    pub fn set_config(&mut self, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64) -> Result<bool, VaultError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.set_config(instant_fee_bps, unbond_period_secs, vesting_period_secs)?;
        }
        self.emit_event(VaultEvent::ConfigChanged { instant_fee_bps, unbond_period_secs, vesting_period_secs }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn pause(&mut self) -> Result<bool, VaultError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.paused = true;
        }
        self.emit_event(VaultEvent::Paused { by: caller }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn resume(&mut self) -> Result<bool, VaultError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.paused = false;
        }
        self.emit_event(VaultEvent::Resumed { by: caller }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn transfer_admin(&mut self, to: ActorId) -> Result<bool, VaultError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            if to.is_zero() { return Err(VaultError::ZeroAddress); }
            s.admin = to;
        }
        self.emit_event(VaultEvent::AdminTransferred { from: caller, to }).expect("event");
        Ok(true)
    }

    // Send accrued instant-exit fees (and swept orphans) to `to`.
    #[export]
    pub async fn collect_fees(&mut self, to: ActorId) -> Result<U256, VaultError> {
        ensure_gas(GAS_ONE_CALL)?;
        let caller = Syscall::message_source();
        let (underlying, amount) = {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            if to.is_zero() { return Err(VaultError::ZeroAddress); }
            let amount = s.take_fees()?;
            (s.underlying, amount)
        };
        match push_underlying(underlying, to, amount).await {
            Ok(()) => {
                self.emit_event(VaultEvent::FeesCollected { to, assets: amount }).expect("event");
                Ok(amount)
            }
            Err(e) => {
                self.state.borrow_mut().restore_fees(amount);
                Err(e)
            }
        }
    }

    // --- sessions --------------------------------------------------------------

    // Propose `key` to act for the caller for `duration_secs` (max 30 days), limited to
    // `actions`. Takes effect once the key calls `accept_session`.
    #[export]
    pub fn create_session(&mut self, key: ActorId, duration_secs: u64, actions: Vec<SessionAction>) -> Result<Session, VaultError> {
        let owner = Syscall::message_source();
        let session = self.state.borrow_mut().propose_session(owner, key, duration_secs, actions, now_ms())?;
        self.emit_event(VaultEvent::SessionProposed { owner, key, expires_at: session.expires_at }).expect("event");
        Ok(session)
    }

    // Sent by the key: accept the session `owner` proposed for it. Replaces the owner's
    // previous session, if any.
    #[export]
    pub fn accept_session(&mut self, owner: ActorId) -> Result<Session, VaultError> {
        let key = Syscall::message_source();
        let session = self.state.borrow_mut().accept_session(key, owner, now_ms())?;
        self.emit_event(VaultEvent::SessionAccepted { owner, key, expires_at: session.expires_at }).expect("event");
        Ok(session)
    }

    // Drop the caller's session (active or proposed). Returns whether one existed.
    #[export]
    pub fn revoke_session(&mut self) -> bool {
        let owner = Syscall::message_source();
        let had = self.state.borrow_mut().revoke_session(owner);
        if had { self.emit_event(VaultEvent::SessionRevoked { owner }).expect("event"); }
        had
    }

    #[export]
    pub fn session(&self, owner: ActorId) -> Option<Session> {
        let s = self.state.borrow();
        s.sessions.get(&owner).filter(|x| now_ms() < x.expires_at).cloned()
    }

    #[export]
    pub fn pending_session(&self, owner: ActorId) -> Option<Session> {
        let s = self.state.borrow();
        s.pending_sessions.get(&owner).filter(|x| now_ms() < x.expires_at).cloned()
    }

    #[export]
    pub fn session_owner(&self, key: ActorId) -> Option<ActorId> {
        let s = self.state.borrow();
        let owner = s.session_keys.get(&key)?;
        s.sessions.get(owner).filter(|x| now_ms() < x.expires_at).map(|_| *owner)
    }

    // --- queries -------------------------------------------------------------

    #[export]
    pub fn info(&self) -> VaultInfo {
        self.state.borrow().info(now_ms())
    }

    // Assets per share scaled by 1e18, at the current block.
    #[export]
    pub fn rate(&self) -> U256 {
        self.state.borrow().rate_at(now_ms())
    }

    #[export]
    pub fn preview_deposit(&self, assets: U256) -> U256 {
        self.state.borrow().shares_for(assets, now_ms()).unwrap_or(U256::zero())
    }

    #[export]
    pub fn preview_redeem(&self, shares: U256) -> RedeemPreview {
        self.state.borrow().preview_redeem(shares, now_ms()).unwrap_or(RedeemPreview { assets: U256::zero(), fee: U256::zero(), net: U256::zero() })
    }

    #[export]
    pub fn position(&self, account: ActorId) -> Position {
        let s = self.state.borrow();
        let shares = s.balance_of(&account);
        Position { shares, assets: s.assets_for(shares, now_ms()).unwrap_or(U256::zero()) }
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
    state: RefCell<VaultState>,
}

#[sails_rs::program]
impl Program {
    // Deploy a vault over `underlying`. The deployer becomes admin. `initial_rate` (1e18 scale,
    // 0 = 1.0) is the share price the first deposit is priced at, for a vault that continues an
    // earlier one. `vesting_period_secs` is how long each rewards tranche takes to vest (an hour to a year).
    pub fn new(underlying: ActorId, name: String, symbol: String, decimals: u8, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64, initial_rate: U256) -> Self {
        let state = VaultState::new(Syscall::message_source(), underlying, name, symbol, decimals, instant_fee_bps, unbond_period_secs, vesting_period_secs, initial_rate);
        Self { state: RefCell::new(state) }
    }

    // Keep `vft` first: `VFT_ROUTE_IDX` relies on it being service #1.
    pub fn vft(&self) -> Vft<'_> {
        Vft::new(&self.state)
    }

    pub fn vault(&self) -> Vault<'_> {
        Vault::new(&self.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(n: u64) -> ActorId { ActorId::from(n) }
    fn u(n: u64) -> U256 { U256::from(n) }
    const T0: u64 = 1_700_000_000_000;
    const DAY: u64 = 86_400 * 1000;
    const WEEK_SECS: u64 = 7 * 86_400;
    const ONE: u64 = 1_000_000;

    fn vault(fee_bps: u32) -> VaultState {
        VaultState::new(a(1), a(99), "k".into(), "kUSDC".into(), 6, fee_bps, WEEK_SECS, WEEK_SECS, U256::zero())
    }

    #[test]
    fn first_deposit_is_one_to_one() {
        let mut v = vault(30);
        assert_eq!(v.check_deposit(u(ONE), T0).unwrap(), u(ONE));
        assert_eq!(v.settle_deposit(a(2), u(ONE), T0).unwrap(), u(ONE));
        assert_eq!(v.balance_of(&a(2)), u(ONE));
        assert_eq!(v.holdings, u(ONE));
        assert_eq!(v.total_assets(T0), u(ONE));
        assert_eq!(v.rate_at(T0), SCALE);
    }

    #[test]
    fn rate_only_moves_as_rewards_vest() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(100 * ONE), T0).unwrap();
        v.fund(u(7 * ONE), T0).unwrap();
        assert_eq!(v.rate_at(T0), SCALE, "nothing vested yet");
        assert_eq!(v.rate_at(T0 + DAY), SCALE + SCALE / 100, "1% after a day");
        assert_eq!(v.rate_at(T0 + 7 * DAY), SCALE + SCALE * 7 / 100, "7% when fully vested");
        assert_eq!(v.rate_at(T0 + 30 * DAY), SCALE + SCALE * 7 / 100, "and no further");
        assert_eq!(v.apy_bps(T0), 36_500);
        assert_eq!(v.apy_bps(T0 + 7 * DAY), 0);
        // A vault with no rewards never moves; time going backwards is harmless.
        let q = vault(0);
        assert_eq!(q.rate_at(T0 + 10_000_000), SCALE);
        assert_eq!(v.rate_at(T0 - 5), SCALE);
    }

    #[test]
    fn instant_round_trip_earns_nothing_and_pays_the_fee() {
        let mut v = vault(30);
        v.settle_deposit(a(2), u(1_000 * ONE), T0).unwrap();
        v.fund(u(84 * ONE), T0).unwrap();
        let t = T0 + 3 * DAY;
        let before = v.distributable(t);
        let shares = v.settle_deposit(a(3), u(100 * ONE), t).unwrap();
        assert!(shares < u(100 * ONE), "priced at the grown rate: {shares}");
        let p = v.begin_redeem(a(3), shares, t).unwrap();
        assert!(p.assets <= u(100 * ONE), "nothing extra in the same block: {}", p.assets);
        assert!(v.distributable(t) >= before, "the vault never loses on a round trip");
        let shares = v.settle_deposit(a(3), u(100 * ONE), t).unwrap();
        let p = v.begin_redeem(a(3), shares, t + 3_000).unwrap();
        assert!(p.assets < u(100 * ONE) + u(ONE / 100), "one block of drip is dust: {}", p.assets);
        assert!(p.net < u(100 * ONE), "the fee dominates: {}", p.net);
    }

    #[test]
    fn deposit_after_growth_gets_fewer_shares() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(100 * ONE), T0).unwrap();
        v.fund(u(10 * ONE), T0).unwrap();
        let t = T0 + 7 * DAY;
        assert_eq!(v.check_deposit(u(110 * ONE), t).unwrap(), u(100 * ONE));
        assert_eq!(v.settle_deposit(a(3), u(110 * ONE), t).unwrap(), u(100 * ONE));
        assert_eq!(v.assets_for(u(100 * ONE), t).unwrap(), u(110 * ONE));
    }

    #[test]
    fn redeem_takes_the_payout_out_of_holdings_before_the_transfer() {
        let mut v = vault(30);
        v.settle_deposit(a(2), u(ONE), T0).unwrap();
        let p = v.begin_redeem(a(2), u(400_000), T0).unwrap();
        assert_eq!(p, RedeemPreview { assets: u(400_000), fee: u(1_200), net: u(398_800) });
        assert_eq!(v.balance_of(&a(2)), u(600_000));
        assert_eq!(v.fees_accrued, u(1_200));
        assert_eq!(v.holdings, u(601_200), "the net payout is already out");
        assert_eq!(v.rate_at(T0), SCALE, "nothing between here and the transfer sees a better rate");
        assert_eq!(v.begin_redeem(a(2), u(600_001), T0), Err(VaultError::InsufficientShares));
        v.revert_redeem(a(2), u(400_000), &p);
        assert_eq!(v.balance_of(&a(2)), u(ONE));
        assert_eq!(v.total_shares, u(ONE));
        assert_eq!(v.fees_accrued, U256::zero());
        assert_eq!(v.holdings, u(ONE));
    }

    #[test]
    fn locked_rewards_are_never_paid_out() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(100 * ONE), T0).unwrap();
        v.fund(u(50 * ONE), T0).unwrap();
        let p = v.begin_redeem(a(2), u(100 * ONE), T0).unwrap();
        assert_eq!(p.assets, u(100 * ONE), "only the principal is distributable on day zero");
        assert_eq!(v.holdings, u(50 * ONE), "the tranche stays in the vault");
        assert_eq!(v.fund(U256::zero(), T0), Err(VaultError::ZeroAmount));
    }

    #[test]
    fn empty_vault_keeps_its_rate_and_orphans_go_to_fees() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(100 * ONE), T0).unwrap();
        v.fund(u(7 * ONE), T0).unwrap();
        let t = T0 + 7 * DAY;
        v.fund(u(70 * ONE), t).unwrap();
        v.begin_redeem(a(2), u(100 * ONE), t).unwrap();
        assert_eq!(v.total_shares, U256::zero());
        assert_eq!(v.rate_at(t), SCALE + SCALE * 7 / 100, "the rate survives the empty spell");
        let t2 = t + 7 * DAY / 2;
        let shares = v.settle_deposit(a(3), u(MIN_DEPOSIT), t2).unwrap();
        assert_eq!(v.fees_accrued, u(35 * ONE), "rewards vested with nobody holding are swept");
        assert_eq!(v.assets_for(shares, t2).unwrap(), u(MIN_DEPOSIT));
        assert!(v.assets_for(shares, t2 + 7 * DAY).unwrap() > u(30 * ONE), "the rest vests to the new holder");
    }

    #[test]
    fn dust_funding_cannot_stretch_a_running_tranche() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(1_000 * ONE), T0).unwrap();
        v.fund(u(700 * ONE), T0).unwrap();
        let end = v.vest_end_ms;
        for i in 0..1_000u64 {
            v.fund(U256::one(), T0 + i * 3_000).unwrap();
        }
        assert!(v.vest_end_ms <= end + 1_000, "end moved by {} ms", v.vest_end_ms as i64 - end as i64);
        // A big top-up mid-way averages the time left by amount.
        let mut q = vault(0);
        q.settle_deposit(a(2), u(1_000 * ONE), T0).unwrap();
        q.fund(u(700 * ONE), T0).unwrap();
        q.fund(u(100 * ONE), T0 + 6 * DAY).unwrap(); // 100 left with a day to go, 100 new with a week
        assert_eq!(q.vest_end_ms, T0 + 6 * DAY + 4 * DAY, "(100*1 + 100*7) / 200 = 4 days");
        assert_eq!(q.vest_amount, u(200 * ONE));
    }

    #[test]
    fn unbond_locks_assets_and_claims_after_period() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(ONE), T0).unwrap();
        v.settle_deposit(a(3), u(ONE), T0).unwrap();
        let e = v.request_unbond(a(2), u(500_000), T0).unwrap();
        assert_eq!(e.assets, u(500_000));
        assert_eq!(e.claimable_at, T0 + 7 * DAY);
        assert_eq!(v.unbonding_total, u(500_000));
        assert_eq!(v.balance_of(&a(2)), u(500_000));
        assert_eq!(v.rate_at(T0), SCALE, "an unbond is rate neutral");
        assert_eq!(v.take_unbond(a(2), e.id, T0 + 1), Err(VaultError::UnbondNotReady { claimable_at: e.claimable_at }));
        assert_eq!(v.take_unbond(a(3), e.id, e.claimable_at), Err(VaultError::UnbondNotFound));
        let taken = v.take_unbond(a(2), e.id, e.claimable_at).unwrap();
        assert_eq!(taken, e);
        assert_eq!(v.holdings, u(1_500_000), "taken out before the transfer");
        assert!(v.unbonds_of(&a(2)).is_empty());
        assert_eq!(v.unbonding_total, U256::zero());
        v.restore_unbond(a(2), taken.clone());
        assert_eq!(v.unbonds_of(&a(2)), vec![taken]);
        assert_eq!(v.unbonding_total, u(500_000));
        assert_eq!(v.holdings, u(2 * ONE));
        // Unbonding positions do not earn.
        v.fund(u(200_000), T0).unwrap();
        assert_eq!(v.assets_for(u(ONE), T0 + 7 * DAY).unwrap(), u(1_133_333), "1.7M distributable over the 1.5M staying shares");
    }

    #[test]
    fn fees_leave_holdings_before_the_transfer() {
        let mut v = vault(30);
        v.settle_deposit(a(2), u(ONE), T0).unwrap();
        v.begin_redeem(a(2), u(ONE), T0).unwrap();
        assert_eq!(v.take_fees().unwrap(), u(3_000));
        assert_eq!((v.fees_accrued, v.holdings), (U256::zero(), U256::zero()));
        assert_eq!(v.take_fees(), Err(VaultError::ZeroAmount));
        v.restore_fees(u(3_000));
        assert_eq!((v.fees_accrued, v.holdings), (u(3_000), u(3_000)));
    }

    #[test]
    fn minimum_deposit_pause_and_config() {
        let mut v = vault(0);
        assert_eq!(v.check_deposit(u(999), T0), Err(VaultError::BelowMinimum { min: u(1_000) }));
        assert_eq!(v.check_deposit(U256::zero(), T0), Err(VaultError::ZeroAmount));
        v.paused = true;
        assert_eq!(v.check_deposit(u(10_000), T0), Err(VaultError::Paused));
        assert_eq!(v.request_unbond(a(2), u(1), T0), Err(VaultError::Paused));
        assert_eq!(v.set_config(MAX_FEE_BPS + 1, 60, 60), Err(VaultError::BadConfig));
        assert_eq!(v.set_config(10, 60, MIN_VESTING_SECS - 1), Err(VaultError::BadConfig));
        v.set_config(10, 60, 3_600).unwrap();
        assert_eq!((v.instant_fee_bps, v.unbond_period_ms, v.vesting_period_ms), (10, 60_000, 3_600_000));
    }

    #[test]
    fn receipt_token_transfers_and_allowances() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(ONE), T0).unwrap();
        v.transfer_shares(a(2), a(3), u(250_000)).unwrap();
        assert_eq!(v.balance_of(&a(3)), u(250_000));
        v.approve_shares(a(3), a(4), u(100_000)).unwrap();
        v.spend_allowance(a(3), a(4), u(60_000)).unwrap();
        assert_eq!(v.allowance(&a(3), &a(4)), u(40_000));
        assert_eq!(v.spend_allowance(a(3), a(4), u(40_001)), Err(VaultError::InsufficientAllowance));
        assert_eq!(v.total_shares, u(ONE), "transfers do not change supply");
    }

    #[test]
    fn sessions_need_the_key_to_accept() {
        let mut v = vault(0);
        let all = vec![SessionAction::Deposit, SessionAction::Redeem, SessionAction::Unbond, SessionAction::Claim];
        assert_eq!(v.actor_for(a(2), SessionAction::Deposit, T0).unwrap(), a(2), "no session: the sender acts for itself");
        assert_eq!(v.propose_session(a(2), a(2), 60, all.clone(), T0), Err(VaultError::BadSession));
        assert_eq!(v.propose_session(a(2), ActorId::zero(), 60, all.clone(), T0), Err(VaultError::BadSession));
        assert_eq!(v.propose_session(a(2), a(9), MAX_SESSION_SECS + 1, all.clone(), T0), Err(VaultError::BadSession));
        assert_eq!(v.propose_session(a(2), a(9), 60, vec![], T0), Err(VaultError::BadSession));
        let s = v.propose_session(a(2), a(9), 60, vec![SessionAction::Deposit, SessionAction::Redeem], T0).unwrap();
        assert_eq!(s.expires_at, T0 + 60_000);
        // Proposed but not accepted: the key is still a plain sender.
        assert_eq!(v.actor_for(a(9), SessionAction::Deposit, T0 + 1).unwrap(), a(9));
        assert_eq!(v.accept_session(a(8), a(2), T0 + 1), Err(VaultError::BadSession));
        v.accept_session(a(9), a(2), T0 + 1).unwrap();
        assert_eq!(v.actor_for(a(9), SessionAction::Deposit, T0 + 1).unwrap(), a(2));
        assert_eq!(v.actor_for(a(9), SessionAction::Claim, T0 + 1), Err(VaultError::SessionNotAllowed));
        assert_eq!(v.actor_for(a(9), SessionAction::Deposit, T0 + 60_000), Err(VaultError::SessionExpired));
        // another owner cannot claim the same key
        assert_eq!(v.propose_session(a(3), a(9), 60, all.clone(), T0), Err(VaultError::BadSession));
        // replacing the session frees the old key
        v.propose_session(a(2), a(10), 60, all.clone(), T0).unwrap();
        assert_eq!(v.actor_for(a(9), SessionAction::Deposit, T0 + 1).unwrap(), a(2), "still active until the new key accepts");
        v.accept_session(a(10), a(2), T0 + 1).unwrap();
        assert_eq!(v.actor_for(a(9), SessionAction::Deposit, T0 + 1).unwrap(), a(9), "old key is a plain sender again");
        assert_eq!(v.actor_for(a(10), SessionAction::Claim, T0 + 1).unwrap(), a(2));
        assert!(v.revoke_session(a(2)));
        assert!(!v.revoke_session(a(2)));
        assert_eq!(v.actor_for(a(10), SessionAction::Deposit, T0 + 1).unwrap(), a(10));
    }

    #[test]
    fn initial_rate_applies_from_the_start() {
        let mut v = VaultState::new(a(1), a(99), "k".into(), "kUSDT".into(), 6, 30, WEEK_SECS, WEEK_SECS, SCALE + SCALE * 18 / 100);
        assert_eq!(v.rate_at(T0), SCALE + SCALE * 18 / 100, "starts at 1.18");
        assert_eq!(v.check_deposit(u(1_180_000), T0).unwrap(), u(ONE));
        v.settle_deposit(a(2), u(1_180_000), T0).unwrap();
        assert_eq!(v.rate_at(T0), SCALE + SCALE * 18 / 100, "and stays there once backed by real assets");
        let below = VaultState::new(a(1), a(99), "k".into(), "kUSDT".into(), 6, 0, 60, 60, SCALE / 2);
        assert_eq!(below.rate_at(T0), SCALE, "a rate below 1.0 is clamped");
    }

    #[test]
    fn info_reports_the_vesting_state() {
        let mut v = vault(30);
        v.settle_deposit(a(2), u(100 * ONE), T0).unwrap();
        v.fund(u(7 * ONE), T0).unwrap();
        let i = v.info(T0 + DAY);
        assert_eq!(i.rate, SCALE + SCALE / 100);
        assert_eq!(i.total_assets, u(101 * ONE));
        assert_eq!(i.holdings, u(107 * ONE));
        assert_eq!(i.distributable, u(101 * ONE));
        assert_eq!(i.locked_rewards, u(6 * ONE));
        assert_eq!(i.vesting_ends_at, T0 + 7 * DAY);
        assert_eq!(i.unbond_period_secs, WEEK_SECS);
        let done = v.info(T0 + 8 * DAY);
        assert_eq!((done.apy_bps, done.vesting_ends_at, done.locked_rewards), (0, 0, U256::zero()));
    }
}
