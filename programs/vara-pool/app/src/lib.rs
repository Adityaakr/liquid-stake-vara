//! Vale Protocol kVARA pool: liquid staking for native VARA.
//!
//! The pool program *is* the receipt token (kVARA): its `Vft` service is the standard
//! fungible token surface over share balances, and its `Pool` service moves VARA in
//! and out. Staking is payable: the VARA attached to `stake` is the deposit.
//!
//! Accounting is asset based (ERC-4626 shape) and nothing is promised that the pool does not
//! hold. The rate (VARA per kVARA, scaled by `1e18`) is `distributable / total_shares`, where
//! `distributable` is the VARA reserve minus collected fees, minus VARA locked for unbonds,
//! minus rewards that are still vesting. Rewards sent through `fund_rewards` vest linearly over
//! the vesting period, so a holder earns exactly the slice of every reward that vested while
//! they held shares: entering and leaving in the same block earns nothing, and the instant
//! exit fee makes the round trip a loss. Pricing carries a virtual share and asset offset
//! (OpenZeppelin's ERC-4626 defence), so nobody can inflate the price per share by emptying
//! the pool and donating.
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
/// Smallest payout: 1 VARA (12 decimals). Vara's existential deposit is 1 VARA and a value
/// transfer that would leave the recipient below it is dropped, so no payout may be smaller.
pub const MIN_PAYOUT: u128 = 1_000_000_000_000;
/// Smallest stake: 2 VARA, so a whole position always clears the payout floor even after the
/// instant fee and rounding.
pub const MIN_STAKE: u128 = 2_000_000_000_000;
/// VARA the pool keeps on its own account, outside the reserve, to pay for its messages.
pub const KEEP_VARA: u128 = 1_000_000_000_000;
pub const MAX_FEE_BPS: u32 = 1_000; // 10%
/// Unbonds mature within a year at most.
pub const MAX_UNBOND_SECS: u64 = 365 * 86_400;
/// Virtual shares priced at `base_rate`, added to every quote (ERC-4626 decimals offset): a
/// donation must be a million times the victim's rounding loss to pay off.
pub const VIRTUAL_SHARES: U256 = U256([1_000_000, 0, 0, 0]);
/// Rewards vest over at least an hour (so one block releases a negligible slice) and at most a year.
pub const MIN_VESTING_SECS: u64 = 3_600;
pub const MAX_VESTING_SECS: u64 = 365 * 86_400;
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
    /// The pool's distributable VARA cannot cover this payout; nothing changed.
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
    /// Session key, duration or action list is invalid, or no matching proposal exists.
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
    /// VARA per kVARA scaled by 1e18: distributable VARA over shares, now.
    pub rate: U256,
    /// Annualised release rate of the rewards currently vesting, over distributable VARA.
    /// Zero once the current tranche has fully vested.
    pub apy_bps: u32,
    pub instant_fee_bps: u32,
    pub unbond_period_secs: u64,
    pub vesting_period_secs: u64,
    pub total_shares: U256,
    /// VARA owed to share holders: the distributable amount.
    pub total_assets: U256,
    /// VARA the pool physically holds (stakes, rewards, fees, unbonds).
    pub reserve: U256,
    /// Reserve minus fees, unbonds and rewards still vesting.
    pub distributable: U256,
    /// Rewards not yet released.
    pub locked_rewards: U256,
    /// When the current tranche is fully vested (0 when nothing is vesting).
    pub vesting_ends_at: u64,
    pub fees_accrued: U256,
    pub unbonding_total: U256,
    pub min_stake: U256,
    pub paused: bool,
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
    /// Rate used while the pool holds no shares: the constructor's initial rate, then the last
    /// rate seen before the pool emptied, so the receipt keeps its price across an empty spell.
    pub base_rate: U256,
    pub instant_fee_bps: u32,
    pub unbond_period_ms: u64,
    pub vesting_period_ms: u64,
    /// VARA held for payouts, in base units. Internal accounting: plain transfers to the
    /// program are invisible here, so nothing outside `stake` and `fund` moves the rate.
    pub reserve: U256,
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

impl PoolState {
    pub fn new(admin: ActorId, name: String, symbol: String, decimals: u8, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64, initial_rate: U256) -> Self {
        Self {
            admin,
            name,
            symbol,
            decimals,
            total_shares: U256::zero(),
            balances: BTreeMap::new(),
            allowances: BTreeMap::new(),
            // A pool that starts at a rate above 1.0 continues where an earlier pool left off.
            base_rate: if initial_rate < SCALE { SCALE } else { initial_rate },
            instant_fee_bps: instant_fee_bps.min(MAX_FEE_BPS),
            unbond_period_ms: unbond_period_secs.min(MAX_UNBOND_SECS).saturating_mul(1000),
            vesting_period_ms: vesting_period_secs.clamp(MIN_VESTING_SECS, MAX_VESTING_SECS).saturating_mul(1000),
            reserve: U256::zero(),
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

    /// VARA that belongs to share holders right now.
    pub fn distributable(&self, now_ms: u64) -> U256 {
        self.reserve.saturating_sub(self.fees_accrued).saturating_sub(self.unbonding_total).saturating_sub(self.locked_rewards(now_ms))
    }

    /// The quote basis: distributable VARA plus the virtual assets, over shares plus the virtual
    /// shares. The virtual pair is priced at `base_rate`, so an empty pool quotes exactly
    /// `base_rate` and a full one is nudged towards it by a millionth of a share.
    fn basis(&self, now_ms: u64) -> (U256, U256) {
        // Rounded up so an empty pool never quotes below `base_rate`.
        let virtual_assets = (VIRTUAL_SHARES.saturating_mul(self.base_rate) + SCALE - U256::one()) / SCALE;
        // With no shares outstanding whatever is distributable belongs to nobody and is swept
        // on the next stake, so it does not price anything.
        let assets = if self.total_shares.is_zero() { U256::zero() } else { self.distributable(now_ms) };
        (assets.saturating_add(virtual_assets), self.total_shares.saturating_add(VIRTUAL_SHARES))
    }

    /// VARA per kVARA scaled by 1e18 at `now_ms`.
    pub fn rate_at(&self, now_ms: u64) -> U256 {
        let (assets, shares) = self.basis(now_ms);
        assets.saturating_mul(SCALE) / shares
    }

    /// Shares minted for `assets` at `now_ms` (rounded down, in the pool's favour).
    pub fn shares_for(&self, assets: U256, now_ms: u64) -> Result<U256, PoolError> {
        let (a, s) = self.basis(now_ms);
        assets.checked_mul(s).ok_or(PoolError::Overflow).map(|v| v / a)
    }

    /// VARA owed for `shares` at `now_ms` (rounded down, in the pool's favour).
    pub fn assets_for(&self, shares: U256, now_ms: u64) -> Result<U256, PoolError> {
        let (a, s) = self.basis(now_ms);
        shares.checked_mul(a).ok_or(PoolError::Overflow).map(|v| v / s)
    }

    pub fn total_assets(&self, now_ms: u64) -> U256 {
        if self.total_shares.is_zero() { U256::zero() } else { self.distributable(now_ms) }
    }

    /// Annualised release rate of the vesting tranche over distributable VARA, in bps.
    pub fn apy_bps(&self, now_ms: u64) -> u32 {
        let d = self.distributable(now_ms);
        if self.vest_amount.is_zero() || now_ms >= self.vest_end_ms || d.is_zero() || self.total_shares.is_zero() { return 0; }
        let span = U256::from(self.vest_end_ms - self.vest_start_ms);
        let bps = self.vest_amount.saturating_mul(U256::from(YEAR_SECS * 1000)).saturating_mul(U256::from(BPS)) / (span.saturating_mul(d));
        if bps > U256::from(u32::MAX) { u32::MAX } else { bps.low_u32() }
    }

    pub fn preview_unstake(&self, shares: U256, now_ms: u64) -> Result<UnstakePreview, PoolError> {
        let assets = self.assets_for(shares, now_ms)?;
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

    /// Burn shares. When the last share goes, the pool remembers its rate for the empty spell.
    pub fn burn_shares(&mut self, from: ActorId, value: U256, now_ms: u64) -> Result<(), PoolError> {
        if value.is_zero() { return Err(PoolError::ZeroAmount); }
        let rate_before = self.rate_at(now_ms);
        self.debit(from, value).map_err(|_| PoolError::InsufficientShares)?;
        self.total_shares = self.total_shares.checked_sub(value).ok_or(PoolError::Overflow)?;
        if self.total_shares.is_zero() && rate_before >= SCALE { self.base_rate = rate_before; }
        Ok(())
    }

    // --- pool flows ---------------------------------------------------------

    /// VARA that belongs to nobody (dust and rewards vested while the pool was empty) goes to
    /// the fee bucket, so the first staker after an empty spell cannot claim it.
    fn sweep_orphans(&mut self, now_ms: u64) {
        if self.total_shares.is_zero() {
            let orphaned = self.distributable(now_ms);
            self.fees_accrued = self.fees_accrued.saturating_add(orphaned);
        }
    }

    /// Stake `assets` of VARA that arrived with the message. Returns the shares minted.
    pub fn stake(&mut self, owner: ActorId, assets: U256, now_ms: u64) -> Result<U256, PoolError> {
        self.ensure_live()?;
        if assets.is_zero() { return Err(PoolError::ZeroAmount); }
        if assets < U256::from(MIN_STAKE) { return Err(PoolError::BelowMinimum { min: U256::from(MIN_STAKE) }); }
        // Nothing is written until every check has passed (the sweep is a write).
        let shares = self.shares_for(assets, now_ms)?;
        if shares.is_zero() { return Err(PoolError::BelowMinimum { min: U256::from(MIN_STAKE) }); }
        self.sweep_orphans(now_ms);
        self.mint_shares(owner, shares)?;
        self.reserve = self.reserve.checked_add(assets).ok_or(PoolError::Overflow)?;
        Ok(shares)
    }

    /// Burn shares for an instant exit and take the payout from the distributable reserve.
    /// Refuses (without changes) when it cannot cover the net amount.
    pub fn unstake(&mut self, owner: ActorId, shares: U256, now_ms: u64) -> Result<UnstakePreview, PoolError> {
        self.ensure_live()?;
        if shares.is_zero() { return Err(PoolError::ZeroAmount); }
        if self.balance_of(&owner) < shares { return Err(PoolError::InsufficientShares); }
        let p = self.preview_unstake(shares, now_ms)?;
        if p.net < U256::from(MIN_PAYOUT) { return Err(PoolError::BelowMinimum { min: U256::from(MIN_PAYOUT) }); }
        let available = self.distributable(now_ms);
        if p.assets > available { return Err(PoolError::InsufficientReserve { available }); }
        self.burn_shares(owner, shares, now_ms)?;
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

    /// Add `assets` of rewards to the vesting tranche. What is still locked from the previous
    /// tranche is folded in, and the new tranche's length is the amount-weighted average of
    /// the time left and a full vesting period (Yearn v3 style): dust cannot stretch a running
    /// tranche, and a new tranche after a finished one gets a full period.
    pub fn fund(&mut self, assets: U256, now_ms: u64) -> Result<(), PoolError> {
        self.ensure_live()?;
        if assets.is_zero() { return Err(PoolError::ZeroAmount); }
        let remaining = self.locked_rewards(now_ms);
        let period = U256::from(self.vesting_period_ms);
        let total = remaining.checked_add(assets).ok_or(PoolError::Overflow)?;
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
        self.reserve = self.reserve.checked_add(assets).ok_or(PoolError::Overflow)?;
        Ok(())
    }

    pub fn request_unbond(&mut self, owner: ActorId, shares: U256, now_ms: u64) -> Result<Unbond, PoolError> {
        self.ensure_live()?;
        if shares.is_zero() { return Err(PoolError::ZeroAmount); }
        if self.balance_of(&owner) < shares { return Err(PoolError::InsufficientShares); }
        let assets = self.assets_for(shares, now_ms)?;
        if assets < U256::from(MIN_PAYOUT) { return Err(PoolError::BelowMinimum { min: U256::from(MIN_PAYOUT) }); }
        let available = self.distributable(now_ms);
        if assets > available { return Err(PoolError::InsufficientReserve { available }); }
        self.burn_shares(owner, shares, now_ms)?;
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

    pub fn set_config(&mut self, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64) -> Result<(), PoolError> {
        if instant_fee_bps > MAX_FEE_BPS || unbond_period_secs > MAX_UNBOND_SECS || !(MIN_VESTING_SECS..=MAX_VESTING_SECS).contains(&vesting_period_secs) { return Err(PoolError::BadConfig); }
        self.instant_fee_bps = instant_fee_bps;
        self.unbond_period_ms = unbond_period_secs.saturating_mul(1000);
        self.vesting_period_ms = vesting_period_secs.saturating_mul(1000);
        Ok(())
    }

    // --- sessions ------------------------------------------------------------

    /// Who a message acts for: the sender itself, or the owner of the accepted session key it
    /// holds. An expired session no longer binds the key: it is a plain account again.
    pub fn actor_for(&self, sender: ActorId, action: SessionAction, now_ms: u64) -> Result<ActorId, PoolError> {
        let Some(owner) = self.session_keys.get(&sender) else { return Ok(sender) };
        let Some(session) = self.sessions.get(owner) else { return Ok(sender) };
        if now_ms >= session.expires_at { return Ok(sender); }
        if !session.actions.contains(&action) { return Err(PoolError::SessionNotAllowed); }
        Ok(*owner)
    }

    /// Propose a session. Nothing changes for `key` until it accepts.
    pub fn propose_session(&mut self, owner: ActorId, key: ActorId, duration_secs: u64, actions: Vec<SessionAction>, now_ms: u64) -> Result<Session, PoolError> {
        if key.is_zero() || key == owner || duration_secs == 0 || duration_secs > MAX_SESSION_SECS || actions.is_empty() { return Err(PoolError::BadSession); }
        // A key may serve one owner.
        if self.key_bound_elsewhere(key, owner, now_ms) { return Err(PoolError::BadSession); }
        let session = Session { key, expires_at: now_ms.saturating_add(duration_secs.saturating_mul(1000)), actions };
        self.pending_sessions.insert(owner, session.clone());
        Ok(session)
    }

    /// True when `key` is held by a live session of another owner.
    fn key_bound_elsewhere(&self, key: ActorId, owner: ActorId, now_ms: u64) -> bool {
        match self.session_keys.get(&key) {
            Some(other) if *other != owner => self.sessions.get(other).is_some_and(|s| now_ms < s.expires_at),
            _ => false,
        }
    }

    /// The key accepts the session `owner` proposed for it; from now on it acts for `owner`.
    /// Replaces any session the owner had before.
    pub fn accept_session(&mut self, key: ActorId, owner: ActorId, now_ms: u64) -> Result<Session, PoolError> {
        let proposed = self.pending_sessions.get(&owner).filter(|s| s.key == key).ok_or(PoolError::BadSession)?;
        if now_ms >= proposed.expires_at { return Err(PoolError::SessionExpired); }
        if self.key_bound_elsewhere(key, owner, now_ms) { return Err(PoolError::BadSession); }
        let session = self.pending_sessions.remove(&owner).ok_or(PoolError::BadSession)?;
        if let Some(old) = self.sessions.remove(&owner) { self.session_keys.remove(&old.key); }
        // An expired binding of this key to somebody else is dead; free it.
        if let Some(other) = self.session_keys.get(&key).copied() { if other != owner { self.sessions.remove(&other); self.session_keys.remove(&key); } }
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

    pub fn info(&self, now_ms: u64) -> PoolInfo {
        let locked = self.locked_rewards(now_ms);
        PoolInfo {
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
            reserve: self.reserve,
            distributable: self.distributable(now_ms),
            locked_rewards: locked,
            vesting_ends_at: if locked.is_zero() { 0 } else { self.vest_end_ms },
            fees_accrued: self.fees_accrued,
            unbonding_total: self.unbonding_total,
            min_stake: U256::from(MIN_STAKE),
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
    AdminTransferred { from: ActorId, to: ActorId },
    Claimed { owner: ActorId, id: u64, assets: U256 },
    ConfigChanged { instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64 },
    FeesCollected { to: ActorId, assets: U256 },
    Paused { by: ActorId },
    Resumed { by: ActorId },
    RewardsFunded { from: ActorId, assets: U256, reserve: U256, vesting_ends_at: u64 },
    SessionAccepted { owner: ActorId, key: ActorId, expires_at: u64 },
    SessionProposed { owner: ActorId, key: ActorId, expires_at: u64 },
    SessionRevoked { owner: ActorId },
    Staked { owner: ActorId, assets: U256, shares: U256, rate: U256 },
    SurplusRescued { to: ActorId, assets: U256 },
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

/// Reply to a command that does not take value: whatever VARA came with the message goes back
/// with the reply instead of sitting on the program outside the reserve.
fn no_value<T>(reply: T) -> CommandReply<T> {
    let value = Syscall::message_value();
    if value > 0 { CommandReply::new(reply).with_value(value) } else { CommandReply::new(reply) }
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
            s.actor_for(Syscall::message_source(), SessionAction::Stake, now).and_then(|owner| s.stake(owner, assets, now).map(|shares| (owner, shares, s.rate_at(now))))
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
    pub fn unstake(&mut self, shares: U256) -> CommandReply<Result<UnstakePreview, PoolError>> {
        no_value(self.do_unstake(shares))
    }

    fn do_unstake(&mut self, shares: U256) -> Result<UnstakePreview, PoolError> {
        let (owner, preview, rate) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Unstake, now)?;
            let rate = s.rate_at(now);
            let p = s.unstake(owner, shares, now)?;
            (owner, p, rate)
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
    pub fn request_unbond(&mut self, shares: U256) -> CommandReply<Result<Unbond, PoolError>> {
        no_value(self.do_request_unbond(shares))
    }

    fn do_request_unbond(&mut self, shares: U256) -> Result<Unbond, PoolError> {
        let (owner, entry) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Unbond, now)?;
            (owner, s.request_unbond(owner, shares, now)?)
        };
        emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
        self.emit_event(PoolEvent::UnbondRequested { owner, id: entry.id, shares, assets: entry.assets, claimable_at: entry.claimable_at }).expect("event");
        Ok(entry)
    }

    // Claim a matured unbond entry. Returns the VARA paid.
    #[export]
    pub fn claim(&mut self, id: u64) -> CommandReply<Result<U256, PoolError>> {
        no_value(self.do_claim(id))
    }

    fn do_claim(&mut self, id: u64) -> Result<U256, PoolError> {
        let (owner, entry) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            let owner = s.actor_for(Syscall::message_source(), SessionAction::Claim, now)?;
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

    // Add the attached VARA to the rewards tranche; it vests linearly over the vesting period.
    // Anyone may fund rewards. Returns the reserve after funding.
    #[export]
    pub fn fund_rewards(&mut self) -> CommandReply<Result<U256, PoolError>> {
        let value = Syscall::message_value();
        let from = Syscall::message_source();
        let assets = U256::from(value);
        let outcome = {
            let mut s = self.state.borrow_mut();
            s.fund(assets, now_ms()).map(|_| (s.reserve, s.vest_end_ms))
        };
        match outcome {
            Ok((reserve, vesting_ends_at)) => {
                self.emit_event(PoolEvent::RewardsFunded { from, assets, reserve, vesting_ends_at }).expect("event");
                CommandReply::new(Ok(reserve))
            }
            Err(e) => CommandReply::new(Err(e)).with_value(value),
        }
    }

    // --- admin ---------------------------------------------------------------

    #[export]
    pub fn set_config(&mut self, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64) -> CommandReply<Result<bool, PoolError>> {
        no_value(self.do_set_config(instant_fee_bps, unbond_period_secs, vesting_period_secs))
    }

    fn do_set_config(&mut self, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64) -> Result<bool, PoolError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.set_config(instant_fee_bps, unbond_period_secs, vesting_period_secs)?;
        }
        self.emit_event(PoolEvent::ConfigChanged { instant_fee_bps, unbond_period_secs, vesting_period_secs }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn pause(&mut self) -> CommandReply<Result<bool, PoolError>> {
        no_value(self.do_pause())
    }

    fn do_pause(&mut self) -> Result<bool, PoolError> {
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
    pub fn resume(&mut self) -> CommandReply<Result<bool, PoolError>> {
        no_value(self.do_resume())
    }

    fn do_resume(&mut self) -> Result<bool, PoolError> {
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
    pub fn transfer_admin(&mut self, to: ActorId) -> CommandReply<Result<bool, PoolError>> {
        no_value(self.do_transfer_admin(to))
    }

    fn do_transfer_admin(&mut self, to: ActorId) -> Result<bool, PoolError> {
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

    // Send accrued instant-exit fees (and swept orphans) to `to`. Fees are paid from the reserve.
    #[export]
    pub fn collect_fees(&mut self, to: ActorId) -> CommandReply<Result<U256, PoolError>> {
        no_value(self.do_collect_fees(to))
    }

    fn do_collect_fees(&mut self, to: ActorId) -> Result<U256, PoolError> {
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

    // Send VARA that sits on the program outside the reserve (value attached to commands that
    // take none, payouts bounced by a program recipient) to `to`. The reserve, the
    // rewards tranche and a keep-alive buffer are never touched.
    #[export]
    pub fn rescue_surplus(&mut self, to: ActorId, assets: U256) -> CommandReply<Result<U256, PoolError>> {
        no_value(self.do_rescue_surplus(to, assets))
    }

    fn do_rescue_surplus(&mut self, to: ActorId, assets: U256) -> Result<U256, PoolError> {
        let caller = Syscall::message_source();
        let available = {
            let s = self.state.borrow();
            s.ensure_admin(caller)?;
            if to.is_zero() { return Err(PoolError::ZeroAddress); }
            if assets.is_zero() { return Err(PoolError::ZeroAmount); }
            // The value attached to this very message is part of the balance but goes back with the reply.
            let free = U256::from(Syscall::value_available()).saturating_sub(U256::from(Syscall::message_value()));
            free.saturating_sub(s.reserve).saturating_sub(U256::from(KEEP_VARA))
        };
        if assets > available { return Err(PoolError::InsufficientReserve { available }); }
        pay_out(to, assets)?;
        self.emit_event(PoolEvent::SurplusRescued { to, assets }).expect("event");
        Ok(assets)
    }

    // --- sessions --------------------------------------------------------------

    // Propose `key` to act for the caller for `duration_secs` (max 30 days), limited to
    // `actions`. Takes effect once the key calls `accept_session`.
    #[export]
    pub fn create_session(&mut self, key: ActorId, duration_secs: u64, actions: Vec<SessionAction>) -> CommandReply<Result<Session, PoolError>> {
        no_value(self.do_create_session(key, duration_secs, actions))
    }

    fn do_create_session(&mut self, key: ActorId, duration_secs: u64, actions: Vec<SessionAction>) -> Result<Session, PoolError> {
        let owner = Syscall::message_source();
        let session = self.state.borrow_mut().propose_session(owner, key, duration_secs, actions, now_ms())?;
        self.emit_event(PoolEvent::SessionProposed { owner, key, expires_at: session.expires_at }).expect("event");
        Ok(session)
    }

    // Sent by the key: accept the session `owner` proposed for it. Replaces the owner's
    // previous session, if any.
    #[export]
    pub fn accept_session(&mut self, owner: ActorId) -> CommandReply<Result<Session, PoolError>> {
        no_value(self.do_accept_session(owner))
    }

    fn do_accept_session(&mut self, owner: ActorId) -> Result<Session, PoolError> {
        let key = Syscall::message_source();
        let session = self.state.borrow_mut().accept_session(key, owner, now_ms())?;
        self.emit_event(PoolEvent::SessionAccepted { owner, key, expires_at: session.expires_at }).expect("event");
        Ok(session)
    }

    // Drop the caller's session (active or proposed). Returns whether one existed.
    #[export]
    pub fn revoke_session(&mut self) -> CommandReply<bool> {
        let owner = Syscall::message_source();
        let had = self.state.borrow_mut().revoke_session(owner);
        if had { self.emit_event(PoolEvent::SessionRevoked { owner }).expect("event"); }
        no_value(had)
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
    pub fn info(&self) -> PoolInfo {
        self.state.borrow().info(now_ms())
    }

    // VARA per kVARA scaled by 1e18, at the current block.
    #[export]
    pub fn rate(&self) -> U256 {
        self.state.borrow().rate_at(now_ms())
    }

    #[export]
    pub fn preview_stake(&self, assets: U256) -> U256 {
        self.state.borrow().shares_for(assets, now_ms()).unwrap_or(U256::zero())
    }

    #[export]
    pub fn preview_unstake(&self, shares: U256) -> UnstakePreview {
        self.state.borrow().preview_unstake(shares, now_ms()).unwrap_or(UnstakePreview { assets: U256::zero(), fee: U256::zero(), net: U256::zero() })
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
    state: RefCell<PoolState>,
}

#[sails_rs::program]
impl Program {
    // Deploy the kVARA pool. The deployer becomes admin. `initial_rate` (1e18 scale, 0 = 1.0)
    // is the VARA per kVARA the first stake is priced at, for a pool that continues an earlier
    // one. `vesting_period_secs` is how long each rewards tranche takes to vest (an hour to a year).
    pub fn new(name: String, symbol: String, decimals: u8, instant_fee_bps: u32, unbond_period_secs: u64, vesting_period_secs: u64, initial_rate: U256) -> Self {
        let state = PoolState::new(Syscall::message_source(), name, symbol, decimals, instant_fee_bps, unbond_period_secs, vesting_period_secs, initial_rate);
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
    const DAY: u64 = 86_400 * 1000;
    const WEEK_SECS: u64 = 7 * 86_400;

    /// Equal up to the virtual offset's dilution (a millionth) plus one unit of rounding.
    fn near(a: U256, b: U256) -> bool {
        let d = if a > b { a - b } else { b - a };
        d <= b / U256::from(1_000_000u64) + U256::one()
    }

    fn pool(fee_bps: u32) -> PoolState {
        PoolState::new(a(1), "Vale kVARA".into(), "kVARA".into(), 12, fee_bps, WEEK_SECS, WEEK_SECS, U256::zero())
    }

    #[test]
    fn first_stake_is_one_to_one_and_grows_the_reserve() {
        let mut p = pool(30);
        assert_eq!(p.stake(a(2), u(100 * ONE), T0).unwrap(), u(100 * ONE));
        assert_eq!(p.balance_of(&a(2)), u(100 * ONE));
        assert_eq!(p.reserve, u(100 * ONE));
        assert_eq!(p.total_assets(T0), u(100 * ONE));
        assert_eq!(p.rate_at(T0), SCALE);
    }

    #[test]
    fn rate_only_moves_as_rewards_vest() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        p.fund(u(7 * ONE), T0).unwrap(); // 1 VARA per day for a week
        assert_eq!(p.rate_at(T0), SCALE, "nothing vested yet");
        assert_eq!(p.locked_rewards(T0), u(7 * ONE));
        assert!(near(p.rate_at(T0 + DAY), SCALE + SCALE / 100), "1% after a day: {}", p.rate_at(T0 + DAY));
        assert!(near(p.rate_at(T0 + 7 * DAY), SCALE + SCALE * 7 / 100), "7% when fully vested");
        assert_eq!(p.rate_at(T0 + 30 * DAY), p.rate_at(T0 + 7 * DAY), "and no further");
        assert_eq!(p.locked_rewards(T0 + 7 * DAY), U256::zero());
        // The annualised rate of the drip over the distributable amount.
        assert_eq!(p.apy_bps(T0), (365 * 100) as u32);
        assert_eq!(p.apy_bps(T0 + 7 * DAY), 0);
    }

    #[test]
    fn instant_round_trip_earns_nothing_and_pays_the_fee() {
        let mut p = pool(30);
        p.stake(a(2), u(1_000 * ONE), T0).unwrap();
        p.fund(u(350 * ONE), T0).unwrap();
        let t = T0 + 3 * DAY;
        let before = p.distributable(t);
        let shares = p.stake(a(3), u(100 * ONE), t).unwrap();
        assert!(shares < u(100 * ONE), "priced at the grown rate: {shares}");
        let out = p.unstake(a(3), shares, t).unwrap();
        assert!(out.assets <= u(100 * ONE), "nothing extra in the same block: {}", out.assets);
        assert_eq!(out.net, out.assets - out.assets * U256::from(30u64) / U256::from(10_000u64));
        assert!(p.distributable(t) >= before, "the pool never loses on a round trip");
        // One block later the visitor gets one block of the drip, far below the fee.
        let shares = p.stake(a(3), u(100 * ONE), t).unwrap();
        let out = p.unstake(a(3), shares, t + 3_000).unwrap();
        assert!(out.assets < u(100 * ONE) + u(ONE / 100), "one block of drip is dust: {}", out.assets);
        assert!(out.net < u(100 * ONE), "the fee dominates: {}", out.net);
    }

    #[test]
    fn stake_after_growth_gets_fewer_shares_and_shares_stay_pro_rata() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        p.fund(u(35 * ONE), T0).unwrap();
        let t = T0 + 7 * DAY;
        assert!(near(p.stake(a(3), u(135 * ONE), t).unwrap(), u(100 * ONE)));
        assert!(near(p.assets_for(u(100 * ONE), t).unwrap(), u(135 * ONE)));
        // A later tranche is split by shares, not by who was there first.
        p.fund(u(27 * ONE), t).unwrap();
        let t2 = t + 7 * DAY;
        assert!(near(p.assets_for(p.balance_of(&a(2)), t2).unwrap(), u(148_500_000_000_000)));
        assert!(near(p.assets_for(p.balance_of(&a(3)), t2).unwrap(), u(148_500_000_000_000)));
    }

    #[test]
    fn unstake_takes_fee_burns_and_draws_the_reserve() {
        let mut p = pool(30);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        let out = p.unstake(a(2), u(40 * ONE), T0).unwrap();
        assert_eq!(out, UnstakePreview { assets: u(40 * ONE), fee: u(40 * ONE * 30 / 10_000), net: u(40 * ONE - 40 * ONE * 30 / 10_000) });
        assert_eq!(p.balance_of(&a(2)), u(60 * ONE));
        assert_eq!(p.fees_accrued, out.fee);
        assert_eq!(p.reserve, u(100 * ONE) - out.net);
        assert_eq!(p.rate_at(T0), SCALE, "fees do not move the rate");
        assert_eq!(p.unstake(a(2), u(60 * ONE + 1), T0), Err(PoolError::InsufficientShares));
        p.revert_unstake(a(2), u(40 * ONE), &out);
        assert_eq!(p.balance_of(&a(2)), u(100 * ONE));
        assert_eq!(p.reserve, u(100 * ONE));
        assert_eq!(p.fees_accrued, U256::zero());
    }

    #[test]
    fn locked_rewards_are_never_paid_out() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        p.fund(u(50 * ONE), T0).unwrap();
        // Only the principal is distributable on day zero.
        let out = p.unstake(a(2), u(100 * ONE), T0).unwrap();
        assert_eq!(out.assets, u(100 * ONE));
        assert_eq!(p.reserve, u(50 * ONE), "the tranche stays in the pool");
        assert_eq!(p.total_shares, U256::zero());
        assert_eq!(p.fund(U256::zero(), T0), Err(PoolError::ZeroAmount));
    }

    #[test]
    fn empty_pool_keeps_its_rate_and_orphans_go_to_fees() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        p.fund(u(7 * ONE), T0).unwrap();
        let t = T0 + 7 * DAY;
        assert!(near(p.rate_at(t), SCALE + SCALE * 7 / 100));
        // A tranche is still vesting when the last holder leaves.
        p.fund(u(70 * ONE), t).unwrap();
        p.unstake(a(2), u(100 * ONE), t).unwrap();
        assert_eq!(p.total_shares, U256::zero());
        assert!(near(p.rate_at(t), SCALE + SCALE * 7 / 100), "the rate survives the empty spell");
        // Half the tranche vested with nobody holding: it is swept, not handed to the next staker.
        let t2 = t + 7 * DAY / 2;
        let shares = p.stake(a(3), u(MIN_STAKE), t2).unwrap();
        assert!(near(p.fees_accrued, u(35 * ONE)), "swept {}", p.fees_accrued);
        assert!(near(p.assets_for(shares, t2).unwrap(), u(MIN_STAKE)), "the new holder owns their stake");
        assert!(p.assets_for(shares, t2 + 7 * DAY).unwrap() > u(30 * ONE), "the rest vests to the new holder");
    }

    #[test]
    fn dust_funding_cannot_stretch_a_running_tranche() {
        let mut p = pool(0);
        p.stake(a(2), u(1_000 * ONE), T0).unwrap();
        p.fund(u(700 * ONE), T0).unwrap();
        let end = p.vest_end_ms;
        for i in 0..1_000u64 {
            p.fund(U256::one(), T0 + i * 3_000).unwrap();
        }
        assert!(p.vest_end_ms <= end + 1_000, "end moved by {} ms", p.vest_end_ms as i64 - end as i64);
        assert!(p.locked_rewards(T0 + 7 * DAY + 1_000).is_zero());
        // A fresh tranche after the last one finished gets a full period again.
        p.fund(u(ONE), T0 + 8 * DAY).unwrap();
        assert_eq!(p.vest_end_ms, T0 + 15 * DAY);
        // A big top-up mid-way averages the time left by amount.
        let mut q = pool(0);
        q.stake(a(2), u(1_000 * ONE), T0).unwrap();
        q.fund(u(700 * ONE), T0).unwrap();
        q.fund(u(100 * ONE), T0 + 6 * DAY).unwrap(); // 100 left with a day to go, 100 new with a week
        assert_eq!(q.vest_end_ms, T0 + 6 * DAY + 4 * DAY, "(100*1 + 100*7) / 200 = 4 days");
        assert_eq!(q.vest_amount, u(200 * ONE));
    }

    #[test]
    fn unbond_locks_assets_and_claims_from_the_reserve_after_the_period() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        p.stake(a(3), u(100 * ONE), T0).unwrap();
        let e = p.request_unbond(a(2), u(50 * ONE), T0).unwrap();
        assert_eq!(e.assets, u(50 * ONE));
        assert_eq!(e.claimable_at, T0 + 7 * DAY);
        assert_eq!(p.unbonding_total, u(50 * ONE));
        assert_eq!(p.balance_of(&a(2)), u(50 * ONE));
        assert_eq!(p.rate_at(T0), SCALE, "an unbond is rate neutral");
        assert_eq!(p.take_unbond(a(2), e.id, T0 + 1), Err(PoolError::UnbondNotReady { claimable_at: e.claimable_at }));
        assert_eq!(p.take_unbond(a(3), e.id, e.claimable_at), Err(PoolError::UnbondNotFound));
        let taken = p.take_unbond(a(2), e.id, e.claimable_at).unwrap();
        assert_eq!(taken, e);
        assert_eq!(p.reserve, u(150 * ONE));
        assert!(p.unbonds_of(&a(2)).is_empty());
        assert_eq!(p.unbonding_total, U256::zero());
        p.restore_unbond(a(2), taken.clone());
        assert_eq!(p.unbonds_of(&a(2)), vec![taken]);
        assert_eq!(p.reserve, u(200 * ONE));
        assert_eq!(p.unbonding_total, u(50 * ONE));
    }

    #[test]
    fn unbonding_positions_do_not_earn() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        p.stake(a(3), u(100 * ONE), T0).unwrap();
        p.request_unbond(a(2), u(100 * ONE), T0).unwrap();
        p.fund(u(10 * ONE), T0).unwrap();
        let t = T0 + 7 * DAY;
        assert!(near(p.assets_for(u(100 * ONE), t).unwrap(), u(110 * ONE)), "the stayer gets the whole tranche");
        assert_eq!(p.unbonds_of(&a(2))[0].assets, u(100 * ONE));
    }

    #[test]
    fn minimum_stake_pause_and_config() {
        let mut p = pool(0);
        assert_eq!(p.stake(a(2), u(MIN_STAKE - 1), T0), Err(PoolError::BelowMinimum { min: u(MIN_STAKE) }));
        assert_eq!(p.stake(a(2), U256::zero(), T0), Err(PoolError::ZeroAmount));
        p.paused = true;
        assert_eq!(p.stake(a(2), u(ONE), T0), Err(PoolError::Paused));
        assert_eq!(p.request_unbond(a(2), u(1), T0), Err(PoolError::Paused));
        assert_eq!(p.set_config(MAX_FEE_BPS + 1, 60, 60), Err(PoolError::BadConfig));
        assert_eq!(p.set_config(10, 60, MIN_VESTING_SECS - 1), Err(PoolError::BadConfig));
        p.set_config(10, 60, 3_600).unwrap();
        assert_eq!((p.instant_fee_bps, p.unbond_period_ms, p.vesting_period_ms), (10, 60_000, 3_600_000));
        let clamped = PoolState::new(a(1), "k".into(), "kVARA".into(), 12, 0, 60, 1, U256::zero());
        assert_eq!(clamped.vesting_period_ms, MIN_VESTING_SECS * 1000);
    }

    #[test]
    fn receipt_token_transfers_and_allowances() {
        let mut p = pool(0);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        p.transfer_shares(a(2), a(3), u(25 * ONE)).unwrap();
        assert_eq!(p.balance_of(&a(3)), u(25 * ONE));
        p.approve_shares(a(3), a(4), u(10 * ONE)).unwrap();
        p.spend_allowance(a(3), a(4), u(6 * ONE)).unwrap();
        assert_eq!(p.allowance(&a(3), &a(4)), u(4 * ONE));
        assert_eq!(p.spend_allowance(a(3), a(4), u(4 * ONE + 1)), Err(PoolError::InsufficientAllowance));
        assert_eq!(p.total_shares, u(100 * ONE), "transfers do not change supply");
    }

    #[test]
    fn sessions_need_the_key_to_accept() {
        let mut p = pool(0);
        let all = vec![SessionAction::Stake, SessionAction::Unstake, SessionAction::Unbond, SessionAction::Claim];
        assert_eq!(p.actor_for(a(2), SessionAction::Stake, T0).unwrap(), a(2));
        assert_eq!(p.propose_session(a(2), a(2), 60, all.clone(), T0), Err(PoolError::BadSession));
        assert_eq!(p.propose_session(a(2), a(9), MAX_SESSION_SECS + 1, all.clone(), T0), Err(PoolError::BadSession));
        let s = p.propose_session(a(2), a(9), 60, vec![SessionAction::Unstake], T0).unwrap();
        assert_eq!(s.expires_at, T0 + 60_000);
        // Proposed but not accepted: the key is still a plain sender. This is what stops an
        // attacker from binding somebody else's account as "their key".
        assert_eq!(p.actor_for(a(9), SessionAction::Unstake, T0 + 1).unwrap(), a(9));
        assert_eq!(p.accept_session(a(8), a(2), T0 + 1), Err(PoolError::BadSession), "only the proposed key can accept");
        assert_eq!(p.accept_session(a(9), a(3), T0 + 1), Err(PoolError::BadSession), "no proposal from that owner");
        p.accept_session(a(9), a(2), T0 + 1).unwrap();
        assert_eq!(p.actor_for(a(9), SessionAction::Unstake, T0 + 1).unwrap(), a(2));
        assert_eq!(p.actor_for(a(9), SessionAction::Claim, T0 + 1), Err(PoolError::SessionNotAllowed));
        assert_eq!(p.actor_for(a(9), SessionAction::Unstake, T0 + 60_000).unwrap(), a(9), "an expired session frees the key");
        // Another owner cannot take a live key, but can take an expired one.
        assert_eq!(p.propose_session(a(3), a(9), 60, all.clone(), T0), Err(PoolError::BadSession));
        assert!(p.propose_session(a(3), a(9), 60, all.clone(), T0 + 60_000).is_ok());
        assert!(p.accept_session(a(9), a(3), T0 + 60_001).is_ok());
        assert_eq!(p.actor_for(a(9), SessionAction::Claim, T0 + 60_002).unwrap(), a(3));
        assert!(p.sessions.get(&a(2)).is_none(), "the dead binding was cleared");
        assert!(p.revoke_session(a(3)));
        p.propose_session(a(2), a(9), 60, vec![SessionAction::Unstake], T0 + 70_000).unwrap();
        p.accept_session(a(9), a(2), T0 + 70_000).unwrap();
        assert!(p.revoke_session(a(2)));
        assert!(!p.revoke_session(a(2)));
        assert_eq!(p.actor_for(a(9), SessionAction::Unstake, T0 + 1).unwrap(), a(9));
        // An expired proposal cannot be accepted.
        p.propose_session(a(2), a(9), 60, all, T0).unwrap();
        assert_eq!(p.accept_session(a(9), a(2), T0 + 60_000), Err(PoolError::SessionExpired));
    }

    #[test]
    fn initial_rate_applies_from_the_start() {
        let mut p = PoolState::new(a(1), "k".into(), "kVARA".into(), 12, 30, WEEK_SECS, WEEK_SECS, SCALE + SCALE / 20);
        assert_eq!(p.rate_at(T0), SCALE + SCALE / 20, "starts at 1.05");
        assert_eq!(p.stake(a(2), u(105 * ONE), T0).unwrap(), u(100 * ONE));
        assert_eq!(p.total_assets(T0), u(105 * ONE));
        assert_eq!(p.rate_at(T0), SCALE + SCALE / 20, "and stays there once backed by real VARA");
        let below = PoolState::new(a(1), "k".into(), "kVARA".into(), 12, 0, 60, 60, SCALE / 2);
        assert_eq!(below.rate_at(T0), SCALE, "a rate below 1.0 is clamped");
    }

    #[test]
    fn a_donation_cannot_inflate_the_price_per_share() {
        // ERC-4626 inflation: drive the supply down to one share, donate, and let the next
        // staker's deposit round against them. The virtual offset makes it a loss for the attacker.
        let mut p = pool(0);
        p.stake(a(2), u(2 * ONE), T0).unwrap();
        let all = p.balance_of(&a(2));
        p.unstake(a(2), all - U256::one(), T0).unwrap_or_else(|e| panic!("leave one share: {e:?}"));
        p.fund(u(1_000 * ONE), T0).unwrap();
        let t = T0 + 7 * DAY;
        // The price per share does rise, but a million virtual shares hold the resolution: the
        // victim's rounding loss is one share's worth, and the attacker's one share owns almost none
        // of the donation.
        let victim = p.stake(a(3), u(1_999 * ONE), t).unwrap();
        let back = p.assets_for(victim, t).unwrap();
        let one_share = p.rate_at(t) / SCALE;
        assert!(back + one_share + U256::one() >= u(1_999 * ONE), "the victim loses at most one share of rounding: {back}");
        let attacker = p.assets_for(p.balance_of(&a(2)), t).unwrap();
        assert!(attacker < u(ONE), "the donation went to the pool, not back to the attacker: {attacker}");
    }

    #[test]
    fn payouts_below_one_vara_are_refused_but_a_whole_position_always_exits() {
        let mut p = pool(MAX_FEE_BPS);
        assert_eq!(p.stake(a(2), u(MIN_STAKE - 1), T0), Err(PoolError::BelowMinimum { min: u(MIN_STAKE) }));
        p.stake(a(2), u(MIN_STAKE), T0).unwrap();
        let all = p.balance_of(&a(2));
        assert_eq!(p.unstake(a(2), all / U256::from(2u64), T0), Err(PoolError::BelowMinimum { min: u(MIN_PAYOUT) }), "half of it minus the fee is below the payout floor");
        assert_eq!(p.request_unbond(a(2), all / U256::from(4u64), T0), Err(PoolError::BelowMinimum { min: u(MIN_PAYOUT) }));
        let out = p.unstake(a(2), all, T0).unwrap();
        assert!(out.net >= u(MIN_PAYOUT), "even at the maximum fee a minimum stake exits whole: {}", out.net);
        p.stake(a(3), u(MIN_STAKE), T0).unwrap();
        assert!(p.request_unbond(a(3), p.balance_of(&a(3)), T0).is_ok(), "and unbonds whole");
    }

    #[test]
    fn constructor_bounds_the_config() {
        let p = PoolState::new(a(1), "k".into(), "kVARA".into(), 12, 20_000, 10 * 365 * 86_400, 1, U256::zero());
        assert_eq!((p.instant_fee_bps, p.unbond_period_ms, p.vesting_period_ms), (MAX_FEE_BPS, MAX_UNBOND_SECS * 1000, MIN_VESTING_SECS * 1000));
        let mut q = pool(0);
        assert_eq!(q.set_config(30, MAX_UNBOND_SECS + 1, 3_600), Err(PoolError::BadConfig));
    }

    #[test]
    fn funding_is_frozen_while_paused() {
        let mut p = pool(0);
        p.paused = true;
        assert_eq!(p.fund(u(ONE), T0), Err(PoolError::Paused));
    }

    #[test]
    fn info_reports_the_vesting_state() {
        let mut p = pool(30);
        p.stake(a(2), u(100 * ONE), T0).unwrap();
        p.fund(u(7 * ONE), T0).unwrap();
        let i = p.info(T0 + DAY);
        assert!(near(i.rate, SCALE + SCALE / 100));
        assert_eq!(i.total_assets, u(101 * ONE));
        assert_eq!(i.reserve, u(107 * ONE));
        assert_eq!(i.distributable, u(101 * ONE));
        assert_eq!(i.locked_rewards, u(6 * ONE));
        assert_eq!(i.vesting_ends_at, T0 + 7 * DAY);
        assert_eq!(i.vesting_period_secs, WEEK_SECS);
        assert_eq!(i.min_stake, u(MIN_STAKE));
        let done = p.info(T0 + 8 * DAY);
        assert_eq!((done.apy_bps, done.vesting_ends_at, done.locked_rewards), (0, 0, U256::zero()));
    }
}
