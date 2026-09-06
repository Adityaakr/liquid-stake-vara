//! Vale Protocol vault: a liquid staking style vault for one underlying VFT.
//!
//! The vault program *is* the receipt token (kUSDC, kUSDT): its `Vft` service is
//! the standard fungible token surface over share balances, and its `Vault`
//! service moves the underlying in and out.
//!
//! Accounting is rate based (ERC-4626 shape). `rate` is assets per share scaled
//! by `1e18`, starts at exactly `1.0` (so the first deposit is 1:1) and grows at
//! the configured APY, checkpointed on every state changing call.
//!
//! Async rules (Gear persists state at every `.await`):
//! 1. validate and checkpoint before the first await, never hold a borrow across it;
//! 2. burn shares or take the unbond entry *before* the outbound transfer, and
//!    compensate on failure, returning `Err` instead of panicking after an await.

#![no_std]

use core::cell::RefCell;
use sails_rs::{collections::BTreeMap, prelude::*};

#[allow(dead_code, unused_imports)]
pub mod token_client;
use sails_rs::client::Program as _;
use token_client::{DemoToken as _, DemoTokenProgram, admin::Admin as _, vft::Vft as _};

/// Route index of the `vft` service inside the program (first service => 1).
const VFT_ROUTE_IDX: u8 = 1;
/// Rate scale: 1e18.
pub const SCALE: U256 = U256([1_000_000_000_000_000_000u64, 0, 0, 0]);
pub const BPS: u64 = 10_000;
pub const YEAR_SECS: u64 = 31_536_000;
/// Smallest deposit in base units so share rounding can never produce zero.
pub const MIN_DEPOSIT: u64 = 1_000;
pub const MAX_APY_BPS: u32 = 100_000; // 1000% (demo ceiling)
pub const MAX_FEE_BPS: u32 = 1_000; // 10%
/// Gas handed to each call into the underlying token (measured ~1.5B on a 1.10 node).
pub const TOKEN_CALL_GAS: u64 = 6_000_000_000;
/// Gas a caller must still have when an async command starts, so the segment that
/// runs after the token reply can never run out of gas with funds already moved.
/// One token call: instantiate again (~17B on a 1.10 node) + call + margin.
pub const GAS_ONE_CALL: u64 = 40_000_000_000;
/// Two token calls (mint shortfall, then transfer).
pub const GAS_TWO_CALLS: u64 = 70_000_000_000;

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
    /// Assets per share scaled by 1e18, projected to now.
    pub rate: U256,
    pub apy_bps: u32,
    pub instant_fee_bps: u32,
    pub unbond_period_secs: u64,
    pub total_shares: U256,
    /// Assets owed to share holders at the projected rate.
    pub total_assets: U256,
    /// Underlying the vault physically holds.
    pub holdings: U256,
    pub fees_accrued: U256,
    pub unbonding_total: U256,
    pub min_deposit: U256,
    pub paused: bool,
    pub last_accrual_at: u64,
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
    pub rate: U256,
    pub last_accrual_ms: u64,
    pub apy_bps: u32,
    pub instant_fee_bps: u32,
    pub unbond_period_ms: u64,
    pub holdings: U256,
    pub fees_accrued: U256,
    pub unbonding_total: U256,
    pub paused: bool,
    pub unbonds: BTreeMap<ActorId, Vec<Unbond>>,
    pub next_unbond_id: u64,
}

impl VaultState {
    pub fn new(admin: ActorId, underlying: ActorId, name: String, symbol: String, decimals: u8, apy_bps: u32, instant_fee_bps: u32, unbond_period_secs: u64, now_ms: u64) -> Self {
        Self {
            admin,
            underlying,
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
            holdings: U256::zero(),
            fees_accrued: U256::zero(),
            unbonding_total: U256::zero(),
            paused: false,
            unbonds: BTreeMap::new(),
            next_unbond_id: 1,
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

    pub fn shares_for(&self, assets: U256) -> Result<U256, VaultError> {
        assets.checked_mul(SCALE).ok_or(VaultError::Overflow).map(|v| v / self.rate)
    }

    pub fn assets_for(&self, shares: U256) -> Result<U256, VaultError> {
        shares.checked_mul(self.rate).ok_or(VaultError::Overflow).map(|v| v / SCALE)
    }

    pub fn total_assets(&self) -> U256 {
        self.assets_for(self.total_shares).unwrap_or(U256::MAX)
    }

    pub fn preview_redeem(&self, shares: U256) -> Result<RedeemPreview, VaultError> {
        let assets = self.assets_for(shares)?;
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

    pub fn burn_shares(&mut self, from: ActorId, value: U256) -> Result<(), VaultError> {
        if value.is_zero() { return Err(VaultError::ZeroAmount); }
        self.debit(from, value).map_err(|_| VaultError::InsufficientShares)?;
        self.total_shares = self.total_shares.checked_sub(value).ok_or(VaultError::Overflow)?;
        Ok(())
    }

    // --- vault flows --------------------------------------------------------

    /// Pre-await checks for a deposit. Returns the shares the deposit would mint now.
    pub fn check_deposit(&self, assets: U256) -> Result<U256, VaultError> {
        self.ensure_live()?;
        if assets.is_zero() { return Err(VaultError::ZeroAmount); }
        if assets < U256::from(MIN_DEPOSIT) { return Err(VaultError::BelowMinimum { min: U256::from(MIN_DEPOSIT) }); }
        let shares = self.shares_for(assets)?;
        if shares.is_zero() { return Err(VaultError::BelowMinimum { min: U256::from(MIN_DEPOSIT) }); }
        Ok(shares)
    }

    /// Post-await settlement of a deposit whose tokens have arrived.
    pub fn settle_deposit(&mut self, owner: ActorId, assets: U256) -> Result<U256, VaultError> {
        let shares = self.shares_for(assets)?.max(U256::one());
        self.mint_shares(owner, shares)?;
        self.holdings = self.holdings.checked_add(assets).ok_or(VaultError::Overflow)?;
        Ok(shares)
    }

    /// Burn shares for an instant exit. Returns the preview used (assets, fee, net).
    pub fn begin_redeem(&mut self, owner: ActorId, shares: U256) -> Result<RedeemPreview, VaultError> {
        self.ensure_live()?;
        if shares.is_zero() { return Err(VaultError::ZeroAmount); }
        if self.balance_of(&owner) < shares { return Err(VaultError::InsufficientShares); }
        let p = self.preview_redeem(shares)?;
        if p.net.is_zero() { return Err(VaultError::ZeroAmount); }
        self.burn_shares(owner, shares)?;
        self.fees_accrued = self.fees_accrued.saturating_add(p.fee);
        Ok(p)
    }

    /// Undo `begin_redeem` after a failed payout.
    pub fn revert_redeem(&mut self, owner: ActorId, shares: U256, p: &RedeemPreview) {
        let _ = self.mint_shares(owner, shares);
        self.fees_accrued = self.fees_accrued.saturating_sub(p.fee);
    }

    /// How much underlying must be minted so the vault can pay `amount` out.
    pub fn shortfall(&self, amount: U256) -> U256 {
        amount.saturating_sub(self.holdings)
    }

    pub fn note_paid_out(&mut self, amount: U256) {
        self.holdings = self.holdings.saturating_sub(amount);
    }

    pub fn note_minted(&mut self, amount: U256) -> Result<(), VaultError> {
        self.holdings = self.holdings.checked_add(amount).ok_or(VaultError::Overflow)?;
        Ok(())
    }

    pub fn request_unbond(&mut self, owner: ActorId, shares: U256, now_ms: u64) -> Result<Unbond, VaultError> {
        self.ensure_live()?;
        if shares.is_zero() { return Err(VaultError::ZeroAmount); }
        if self.balance_of(&owner) < shares { return Err(VaultError::InsufficientShares); }
        let assets = self.assets_for(shares)?;
        if assets.is_zero() { return Err(VaultError::ZeroAmount); }
        self.burn_shares(owner, shares)?;
        self.unbonding_total = self.unbonding_total.checked_add(assets).ok_or(VaultError::Overflow)?;
        let entry = Unbond { id: self.next_unbond_id, shares, assets, requested_at: now_ms, claimable_at: now_ms.saturating_add(self.unbond_period_ms) };
        self.next_unbond_id += 1;
        self.unbonds.entry(owner).or_default().push(entry.clone());
        Ok(entry)
    }

    /// Remove a claimable unbond entry (pre-await).
    pub fn take_unbond(&mut self, owner: ActorId, id: u64, now_ms: u64) -> Result<Unbond, VaultError> {
        self.ensure_live()?;
        let list = self.unbonds.get_mut(&owner).ok_or(VaultError::UnbondNotFound)?;
        let idx = list.iter().position(|u| u.id == id).ok_or(VaultError::UnbondNotFound)?;
        if now_ms < list[idx].claimable_at { return Err(VaultError::UnbondNotReady { claimable_at: list[idx].claimable_at }); }
        let entry = list.remove(idx);
        if list.is_empty() { self.unbonds.remove(&owner); }
        self.unbonding_total = self.unbonding_total.saturating_sub(entry.assets);
        Ok(entry)
    }

    /// Put an unbond entry back after a failed payout.
    pub fn restore_unbond(&mut self, owner: ActorId, entry: Unbond) {
        self.unbonding_total = self.unbonding_total.saturating_add(entry.assets);
        let list = self.unbonds.entry(owner).or_default();
        list.push(entry);
        list.sort_by_key(|u| u.id);
    }

    pub fn unbonds_of(&self, owner: &ActorId) -> Vec<Unbond> {
        self.unbonds.get(owner).cloned().unwrap_or_default()
    }

    fn ensure_admin(&self, who: ActorId) -> Result<(), VaultError> {
        if self.admin == who { Ok(()) } else { Err(VaultError::Unauthorized) }
    }

    pub fn info(&self, now_ms: u64) -> VaultInfo {
        let rate = self.rate_at(now_ms);
        let total_assets = self.total_shares.checked_mul(rate).map(|v| v / SCALE).unwrap_or(U256::MAX);
        VaultInfo {
            underlying: self.underlying,
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
            holdings: self.holdings,
            fees_accrued: self.fees_accrued,
            unbonding_total: self.unbonding_total,
            min_deposit: U256::from(MIN_DEPOSIT),
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
    Accrued { rate: U256, total_assets: U256 },
    AdminTransferred { from: ActorId, to: ActorId },
    Claimed { owner: ActorId, id: u64, assets: U256 },
    ConfigChanged { apy_bps: u32, instant_fee_bps: u32, unbond_period_secs: u64 },
    Deposited { owner: ActorId, assets: U256, shares: U256, rate: U256 },
    FeesCollected { to: ActorId, assets: U256 },
    Paused { by: ActorId },
    Redeemed { owner: ActorId, shares: U256, assets: U256, fee: U256, rate: U256 },
    ReserveToppedUp { assets: U256 },
    Resumed { by: ActorId },
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

/// Mint `assets` of the underlying to the vault itself (the vault must be a minter).
async fn mint_underlying_to_self(underlying: ActorId, assets: U256) -> Result<(), VaultError> {
    let me = Syscall::program_id();
    match DemoTokenProgram::client(underlying).admin().mint(me, assets).with_gas_limit(TOKEN_CALL_GAS).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => Err(VaultError::TokenRejected(format!("{e:?}"))),
        Err(e) => Err(VaultError::TokenCall(format!("{e:?}"))),
    }
}

#[sails_rs::service(events = VaultEvent)]
impl Vault<'_> {
    /// Cover a payout: mint the shortfall (realised yield) to the vault when holdings are short.
    async fn ensure_holdings(&self, amount: U256) -> Result<(), VaultError> {
        let (underlying, short) = {
            let s = self.state.borrow();
            (s.underlying, s.shortfall(amount))
        };
        if short.is_zero() { return Ok(()); }
        mint_underlying_to_self(underlying, short).await?;
        self.state.borrow_mut().note_minted(short)
    }

    // Deposit `assets` of the underlying (requires prior approval). Returns shares minted.
    #[export]
    pub async fn deposit(&mut self, assets: U256) -> Result<U256, VaultError> {
        ensure_gas(GAS_ONE_CALL)?;
        let owner = Syscall::message_source();
        let underlying = {
            let mut s = self.state.borrow_mut();
            s.accrue(now_ms());
            s.check_deposit(assets)?;
            s.underlying
        };
        pull_underlying(underlying, owner, assets).await?;
        let (shares, rate) = {
            let mut s = self.state.borrow_mut();
            s.accrue(now_ms());
            let shares = s.settle_deposit(owner, assets)?;
            (shares, s.rate)
        };
        emit_vft_transfer(self.state, ActorId::zero(), owner, shares);
        self.emit_event(VaultEvent::Deposited { owner, assets, shares, rate }).expect("event");
        Ok(shares)
    }

    // Instant exit: burn `shares`, receive assets at the current rate minus the instant fee.
    #[export]
    pub async fn redeem(&mut self, shares: U256) -> Result<RedeemPreview, VaultError> {
        ensure_gas(GAS_TWO_CALLS)?;
        let owner = Syscall::message_source();
        let (underlying, preview, rate) = {
            let mut s = self.state.borrow_mut();
            s.accrue(now_ms());
            let p = s.begin_redeem(owner, shares)?;
            (s.underlying, p, s.rate)
        };
        emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
        let paid = async {
            self.ensure_holdings(preview.net).await?;
            push_underlying(underlying, owner, preview.net).await
        }
        .await;
        match paid {
            Ok(()) => {
                self.state.borrow_mut().note_paid_out(preview.net);
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
        let owner = Syscall::message_source();
        let entry = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            s.accrue(now);
            s.request_unbond(owner, shares, now)?
        };
        emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
        self.emit_event(VaultEvent::UnbondRequested { owner, id: entry.id, shares, assets: entry.assets, claimable_at: entry.claimable_at }).expect("event");
        Ok(entry)
    }

    // Claim a matured unbond entry. Returns the assets paid.
    #[export]
    pub async fn claim(&mut self, id: u64) -> Result<U256, VaultError> {
        ensure_gas(GAS_TWO_CALLS)?;
        let owner = Syscall::message_source();
        let (underlying, entry) = {
            let mut s = self.state.borrow_mut();
            let now = now_ms();
            s.accrue(now);
            let e = s.take_unbond(owner, id, now)?;
            (s.underlying, e)
        };
        let paid = async {
            self.ensure_holdings(entry.assets).await?;
            push_underlying(underlying, owner, entry.assets).await
        }
        .await;
        match paid {
            Ok(()) => {
                self.state.borrow_mut().note_paid_out(entry.assets);
                self.emit_event(VaultEvent::Claimed { owner, id, assets: entry.assets }).expect("event");
                Ok(entry.assets)
            }
            Err(e) => {
                self.state.borrow_mut().restore_unbond(owner, entry);
                Err(e)
            }
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
        if moved { self.emit_event(VaultEvent::Accrued { rate, total_assets }).expect("event"); }
        rate
    }

    // --- admin ---------------------------------------------------------------

    #[export]
    pub fn set_config(&mut self, apy_bps: u32, instant_fee_bps: u32, unbond_period_secs: u64) -> Result<bool, VaultError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            if apy_bps > MAX_APY_BPS || instant_fee_bps > MAX_FEE_BPS { return Err(VaultError::BadConfig); }
            // Settle the old APY up to now before switching.
            s.accrue(now_ms());
            s.apy_bps = apy_bps;
            s.instant_fee_bps = instant_fee_bps;
            s.unbond_period_ms = unbond_period_secs.saturating_mul(1000);
        }
        self.emit_event(VaultEvent::ConfigChanged { apy_bps, instant_fee_bps, unbond_period_secs }).expect("event");
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

    // Mint `assets` of the underlying into the vault so payouts do not need a lazy mint.
    #[export]
    pub async fn top_up_reserve(&mut self, assets: U256) -> Result<bool, VaultError> {
        ensure_gas(GAS_ONE_CALL)?;
        let caller = Syscall::message_source();
        let underlying = {
            let s = self.state.borrow();
            s.ensure_admin(caller)?;
            if assets.is_zero() { return Err(VaultError::ZeroAmount); }
            s.underlying
        };
        mint_underlying_to_self(underlying, assets).await?;
        self.state.borrow_mut().note_minted(assets)?;
        self.emit_event(VaultEvent::ReserveToppedUp { assets }).expect("event");
        Ok(true)
    }

    // Send accrued instant-exit fees to `to`.
    #[export]
    pub async fn collect_fees(&mut self, to: ActorId) -> Result<U256, VaultError> {
        ensure_gas(GAS_TWO_CALLS)?;
        let caller = Syscall::message_source();
        let (underlying, amount) = {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            if to.is_zero() { return Err(VaultError::ZeroAddress); }
            let amount = s.fees_accrued;
            if amount.is_zero() { return Err(VaultError::ZeroAmount); }
            s.fees_accrued = U256::zero();
            (s.underlying, amount)
        };
        let paid = async {
            self.ensure_holdings(amount).await?;
            push_underlying(underlying, to, amount).await
        }
        .await;
        match paid {
            Ok(()) => {
                self.state.borrow_mut().note_paid_out(amount);
                self.emit_event(VaultEvent::FeesCollected { to, assets: amount }).expect("event");
                Ok(amount)
            }
            Err(e) => {
                self.state.borrow_mut().fees_accrued = amount;
                Err(e)
            }
        }
    }

    // --- queries -------------------------------------------------------------

    #[export]
    pub fn info(&self) -> VaultInfo {
        self.state.borrow().info(now_ms())
    }

    // Assets per share scaled by 1e18, projected to the current block.
    #[export]
    pub fn rate(&self) -> U256 {
        self.state.borrow().rate_at(now_ms())
    }

    #[export]
    pub fn preview_deposit(&self, assets: U256) -> U256 {
        let s = self.state.borrow();
        let rate = s.rate_at(now_ms());
        assets.checked_mul(SCALE).map(|v| v / rate).unwrap_or(U256::zero())
    }

    #[export]
    pub fn preview_redeem(&self, shares: U256) -> RedeemPreview {
        let s = self.state.borrow();
        let rate = s.rate_at(now_ms());
        let assets = shares.checked_mul(rate).map(|v| v / SCALE).unwrap_or(U256::zero());
        let fee = assets.saturating_mul(U256::from(s.instant_fee_bps)) / U256::from(BPS);
        RedeemPreview { assets, fee, net: assets - fee }
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
    state: RefCell<VaultState>,
}

#[sails_rs::program]
impl Program {
    // Deploy a vault over `underlying`. The deployer becomes admin and must be granted the
    // minter role on the underlying so realised yield can be minted.
    pub fn new(underlying: ActorId, name: String, symbol: String, decimals: u8, apy_bps: u32, instant_fee_bps: u32, unbond_period_secs: u64) -> Self {
        let state = VaultState::new(Syscall::message_source(), underlying, name, symbol, decimals, apy_bps, instant_fee_bps, unbond_period_secs, Syscall::block_timestamp());
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

    fn vault(apy_bps: u32) -> VaultState {
        VaultState::new(a(1), a(99), "k".into(), "kUSDC".into(), 6, apy_bps, 30, 7 * 86_400, T0)
    }

    #[test]
    fn first_deposit_is_one_to_one() {
        let mut v = vault(840);
        assert_eq!(v.check_deposit(u(1_000_000)).unwrap(), u(1_000_000));
        assert_eq!(v.settle_deposit(a(2), u(1_000_000)).unwrap(), u(1_000_000));
        assert_eq!(v.balance_of(&a(2)), u(1_000_000));
        assert_eq!(v.holdings, u(1_000_000));
        assert_eq!(v.total_assets(), u(1_000_000));
    }

    #[test]
    fn rate_accrues_linearly_between_checkpoints() {
        let mut v = vault(1_000); // 10% APY
        v.settle_deposit(a(2), u(1_000_000)).unwrap();
        let later = T0 + YEAR_SECS * 1000; // one year
        let r = v.rate_at(later);
        assert_eq!(r, SCALE + SCALE / 10, "10% after a year");
        assert!(v.accrue(later));
        assert_eq!(v.assets_for(u(1_000_000)).unwrap(), u(1_100_000));
        // second year compounds on the checkpointed rate
        let r2 = v.rate_at(later + YEAR_SECS * 1000);
        assert_eq!(r2, r + r / 10);
    }

    #[test]
    fn zero_apy_and_time_going_backwards_do_not_move_rate() {
        let mut v = vault(0);
        assert_eq!(v.rate_at(T0 + 10_000_000), SCALE);
        v.apy_bps = 5_000;
        assert_eq!(v.rate_at(T0 - 5), SCALE);
        assert!(!v.accrue(T0 - 5));
    }

    #[test]
    fn deposit_after_growth_gets_fewer_shares() {
        let mut v = vault(1_000);
        v.settle_deposit(a(2), u(1_000_000)).unwrap();
        v.accrue(T0 + YEAR_SECS * 1000);
        let shares = v.check_deposit(u(1_100_000)).unwrap();
        assert_eq!(shares, u(1_000_000));
    }

    #[test]
    fn redeem_takes_fee_and_burns() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(1_000_000)).unwrap();
        let p = v.begin_redeem(a(2), u(400_000)).unwrap();
        assert_eq!(p, RedeemPreview { assets: u(400_000), fee: u(1_200), net: u(398_800) });
        assert_eq!(v.balance_of(&a(2)), u(600_000));
        assert_eq!(v.fees_accrued, u(1_200));
        assert_eq!(v.shortfall(p.net), U256::zero());
        v.note_paid_out(p.net);
        assert_eq!(v.holdings, u(601_200));
        assert_eq!(v.begin_redeem(a(2), u(600_001)), Err(VaultError::InsufficientShares));
    }

    #[test]
    fn revert_redeem_restores_shares_and_fees() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(1_000_000)).unwrap();
        let p = v.begin_redeem(a(2), u(1_000_000)).unwrap();
        v.revert_redeem(a(2), u(1_000_000), &p);
        assert_eq!(v.balance_of(&a(2)), u(1_000_000));
        assert_eq!(v.total_shares, u(1_000_000));
        assert_eq!(v.fees_accrued, U256::zero());
    }

    #[test]
    fn shortfall_is_the_realised_yield() {
        let mut v = vault(1_000);
        v.settle_deposit(a(2), u(1_000_000)).unwrap();
        v.accrue(T0 + YEAR_SECS * 1000);
        let p = v.begin_redeem(a(2), u(1_000_000)).unwrap();
        assert_eq!(p.assets, u(1_100_000));
        assert_eq!(v.shortfall(p.net), p.net - u(1_000_000));
        v.note_minted(v.shortfall(p.net)).unwrap();
        v.note_paid_out(p.net);
        // Fees are backed lazily too: collect_fees mints its own shortfall.
        assert_eq!(v.holdings, U256::zero());
        assert_eq!(v.fees_accrued, p.fee);
    }

    #[test]
    fn unbond_locks_assets_and_claims_after_period() {
        let mut v = vault(1_000);
        v.settle_deposit(a(2), u(1_000_000)).unwrap();
        let e = v.request_unbond(a(2), u(500_000), T0).unwrap();
        assert_eq!(e.assets, u(500_000));
        assert_eq!(e.claimable_at, T0 + 7 * 86_400 * 1000);
        assert_eq!(v.unbonding_total, u(500_000));
        assert_eq!(v.balance_of(&a(2)), u(500_000));
        assert_eq!(v.take_unbond(a(2), e.id, T0 + 1), Err(VaultError::UnbondNotReady { claimable_at: e.claimable_at }));
        assert_eq!(v.take_unbond(a(3), e.id, e.claimable_at), Err(VaultError::UnbondNotFound));
        let taken = v.take_unbond(a(2), e.id, e.claimable_at).unwrap();
        assert_eq!(taken, e);
        assert!(v.unbonds_of(&a(2)).is_empty());
        assert_eq!(v.unbonding_total, U256::zero());
        v.restore_unbond(a(2), taken.clone());
        assert_eq!(v.unbonds_of(&a(2)), vec![taken]);
        assert_eq!(v.unbonding_total, u(500_000));
    }

    #[test]
    fn minimum_deposit_and_pause() {
        let mut v = vault(0);
        assert_eq!(v.check_deposit(u(999)), Err(VaultError::BelowMinimum { min: u(1_000) }));
        assert_eq!(v.check_deposit(U256::zero()), Err(VaultError::ZeroAmount));
        v.paused = true;
        assert_eq!(v.check_deposit(u(10_000)), Err(VaultError::Paused));
        assert_eq!(v.request_unbond(a(2), u(1), T0), Err(VaultError::Paused));
    }

    #[test]
    fn receipt_token_transfers_and_allowances() {
        let mut v = vault(0);
        v.settle_deposit(a(2), u(1_000_000)).unwrap();
        v.transfer_shares(a(2), a(3), u(250_000)).unwrap();
        assert_eq!(v.balance_of(&a(3)), u(250_000));
        v.approve_shares(a(3), a(4), u(100_000)).unwrap();
        v.spend_allowance(a(3), a(4), u(60_000)).unwrap();
        assert_eq!(v.allowance(&a(3), &a(4)), u(40_000));
        assert_eq!(v.spend_allowance(a(3), a(4), u(40_001)), Err(VaultError::InsufficientAllowance));
        assert_eq!(v.total_shares, u(1_000_000), "transfers do not change supply");
    }

    #[test]
    fn info_projects_rate() {
        let mut v = vault(1_000);
        v.settle_deposit(a(2), u(1_000_000)).unwrap();
        let i = v.info(T0 + YEAR_SECS * 1000);
        assert_eq!(i.rate, SCALE + SCALE / 10);
        assert_eq!(i.total_assets, u(1_100_000));
        assert_eq!(i.holdings, u(1_000_000));
        assert_eq!(i.unbond_period_secs, 7 * 86_400);
    }
}
