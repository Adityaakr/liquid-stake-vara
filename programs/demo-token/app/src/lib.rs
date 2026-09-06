//! Vale Protocol demo token.
//!
//! A standard Vara Fungible Token (VFT) with two extras that a demo needs:
//!
//! * an `Admin` service with a minter role, so the vault can mint yield, and
//! * a public `Faucet` so anyone can pick up test balance on mainnet.
//!
//! The `Vft` service exposes exactly the VFT standard surface (routes and
//! events), so wallets, explorers and `vara-wallet vft ...` treat it like any
//! other token on Vara.

#![no_std]

use core::cell::RefCell;
use sails_rs::{
    collections::{BTreeMap, BTreeSet},
    prelude::*,
};

/// Route index of the `vft` service inside the program (first service => 1).
/// Used to emit VFT `Transfer` events for mints and burns from other services.
const VFT_ROUTE_IDX: u8 = 1;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct TokenState {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: U256,
    pub balances: BTreeMap<ActorId, U256>,
    pub allowances: BTreeMap<(ActorId, ActorId), U256>,
    pub admin: ActorId,
    pub minters: BTreeSet<ActorId>,
    pub paused: bool,
    pub faucet_amount: U256,
    pub faucet_cooldown_ms: u64,
    pub last_claim: BTreeMap<ActorId, u64>,
}

#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    Paused,
    ZeroAddress,
    ZeroAmount,
    InsufficientBalance,
    InsufficientAllowance,
    Unauthorized,
    Overflow,
    FaucetDisabled,
    // Cooldown still running; `next_claim_at` is a unix timestamp in milliseconds.
    FaucetCooldown { next_claim_at: u64 },
}

#[sails_rs::sails_type]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaucetConfig {
    pub amount: U256,
    pub cooldown_secs: u64,
}

// ---------------------------------------------------------------------------
// Pure state transitions (unit tested without a runtime)
// ---------------------------------------------------------------------------

impl TokenState {
    pub fn balance_of(&self, who: &ActorId) -> U256 {
        self.balances.get(who).copied().unwrap_or_default()
    }

    pub fn allowance(&self, owner: &ActorId, spender: &ActorId) -> U256 {
        self.allowances.get(&(*owner, *spender)).copied().unwrap_or_default()
    }

    fn ensure_live(&self) -> Result<(), TokenError> {
        if self.paused { Err(TokenError::Paused) } else { Ok(()) }
    }

    fn credit(&mut self, to: ActorId, value: U256) -> Result<(), TokenError> {
        let bal = self.balance_of(&to);
        let new = bal.checked_add(value).ok_or(TokenError::Overflow)?;
        self.balances.insert(to, new);
        Ok(())
    }

    fn debit(&mut self, from: ActorId, value: U256) -> Result<(), TokenError> {
        let bal = self.balance_of(&from);
        let new = bal.checked_sub(value).ok_or(TokenError::InsufficientBalance)?;
        if new.is_zero() { self.balances.remove(&from); } else { self.balances.insert(from, new); }
        Ok(())
    }

    pub fn transfer(&mut self, from: ActorId, to: ActorId, value: U256) -> Result<(), TokenError> {
        self.ensure_live()?;
        if to.is_zero() { return Err(TokenError::ZeroAddress); }
        if value.is_zero() { return Err(TokenError::ZeroAmount); }
        self.debit(from, value)?;
        self.credit(to, value)
    }

    pub fn approve(&mut self, owner: ActorId, spender: ActorId, value: U256) -> Result<(), TokenError> {
        self.ensure_live()?;
        if spender.is_zero() { return Err(TokenError::ZeroAddress); }
        if value.is_zero() { self.allowances.remove(&(owner, spender)); } else { self.allowances.insert((owner, spender), value); }
        Ok(())
    }

    pub fn spend_allowance(&mut self, owner: ActorId, spender: ActorId, value: U256) -> Result<(), TokenError> {
        let current = self.allowance(&owner, &spender);
        if current == U256::MAX { return Ok(()); }
        let rest = current.checked_sub(value).ok_or(TokenError::InsufficientAllowance)?;
        if rest.is_zero() { self.allowances.remove(&(owner, spender)); } else { self.allowances.insert((owner, spender), rest); }
        Ok(())
    }

    pub fn mint(&mut self, to: ActorId, value: U256) -> Result<(), TokenError> {
        self.ensure_live()?;
        if to.is_zero() { return Err(TokenError::ZeroAddress); }
        if value.is_zero() { return Err(TokenError::ZeroAmount); }
        let supply = self.total_supply.checked_add(value).ok_or(TokenError::Overflow)?;
        self.credit(to, value)?;
        self.total_supply = supply;
        Ok(())
    }

    pub fn burn(&mut self, from: ActorId, value: U256) -> Result<(), TokenError> {
        self.ensure_live()?;
        if value.is_zero() { return Err(TokenError::ZeroAmount); }
        self.debit(from, value)?;
        self.total_supply = self.total_supply.checked_sub(value).ok_or(TokenError::Overflow)?;
        Ok(())
    }

    pub fn faucet_claim(&mut self, who: ActorId, now_ms: u64) -> Result<U256, TokenError> {
        self.ensure_live()?;
        if self.faucet_amount.is_zero() { return Err(TokenError::FaucetDisabled); }
        if let Some(last) = self.last_claim.get(&who) {
            let next = last.saturating_add(self.faucet_cooldown_ms);
            if now_ms < next { return Err(TokenError::FaucetCooldown { next_claim_at: next }); }
        }
        let amount = self.faucet_amount;
        self.mint(who, amount)?;
        self.last_claim.insert(who, now_ms);
        Ok(amount)
    }

    pub fn next_claim_at(&self, who: &ActorId) -> u64 {
        self.last_claim.get(who).map(|l| l.saturating_add(self.faucet_cooldown_ms)).unwrap_or(0)
    }

    fn ensure_admin(&self, who: ActorId) -> Result<(), TokenError> {
        if self.admin == who { Ok(()) } else { Err(TokenError::Unauthorized) }
    }

    fn ensure_minter(&self, who: ActorId) -> Result<(), TokenError> {
        if self.admin == who || self.minters.contains(&who) { Ok(()) } else { Err(TokenError::Unauthorized) }
    }
}

// ---------------------------------------------------------------------------
// Vft service: the VFT standard surface
// ---------------------------------------------------------------------------

#[sails_rs::sails_type]
#[sails_rs::event]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VftEvent {
    Approval { owner: ActorId, spender: ActorId, value: U256 },
    Transfer { from: ActorId, to: ActorId, value: U256 },
}

pub struct Vft<'a> {
    state: &'a RefCell<TokenState>,
}

impl<'a> Vft<'a> {
    pub fn new(state: &'a RefCell<TokenState>) -> Self {
        Self { state }
    }
}

#[sails_rs::service(events = VftEvent)]
impl Vft<'_> {
    #[export(unwrap_result)]
    pub fn approve(&mut self, spender: ActorId, value: U256) -> Result<bool, TokenError> {
        let owner = Syscall::message_source();
        self.state.borrow_mut().approve(owner, spender, value)?;
        self.emit_event(VftEvent::Approval { owner, spender, value }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn transfer(&mut self, to: ActorId, value: U256) -> Result<bool, TokenError> {
        let from = Syscall::message_source();
        self.state.borrow_mut().transfer(from, to, value)?;
        self.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn transfer_from(&mut self, from: ActorId, to: ActorId, value: U256) -> Result<bool, TokenError> {
        let spender = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            if spender != from { s.spend_allowance(from, spender, value)?; }
            s.transfer(from, to, value)?;
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
        self.state.borrow().total_supply
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

/// Emit a VFT `Transfer` event on behalf of the `vft` service from another service.
fn emit_vft_transfer(state: &RefCell<TokenState>, from: ActorId, to: ActorId, value: U256) {
    use sails_rs::gstd::services::Service as _;
    let vft = Vft::new(state).expose(VFT_ROUTE_IDX);
    vft.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
}

// ---------------------------------------------------------------------------
// Admin service: roles, supply, pause, faucet configuration
// ---------------------------------------------------------------------------

#[sails_rs::sails_type]
#[sails_rs::event]
#[derive(Debug, Clone, PartialEq, Eq)]
// Keep variants in alphabetical order: the IDL generator sorts events, while the
// interface hash follows declaration order. They must agree or clients reject the IDL.
pub enum AdminEvent {
    AdminTransferred { from: ActorId, to: ActorId },
    Burned { from: ActorId, value: U256 },
    FaucetConfigured { amount: U256, cooldown_secs: u64 },
    Minted { to: ActorId, value: U256 },
    MinterGranted { account: ActorId },
    MinterRevoked { account: ActorId },
    Paused { by: ActorId },
    Resumed { by: ActorId },
}

pub struct Admin<'a> {
    state: &'a RefCell<TokenState>,
}

impl<'a> Admin<'a> {
    pub fn new(state: &'a RefCell<TokenState>) -> Self {
        Self { state }
    }
}

#[sails_rs::service(events = AdminEvent)]
impl Admin<'_> {
    // Mint `value` to `to`. Caller must be the admin or hold the minter role.
    #[export(unwrap_result)]
    pub fn mint(&mut self, to: ActorId, value: U256) -> Result<bool, TokenError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_minter(caller)?;
            s.mint(to, value)?;
        }
        emit_vft_transfer(self.state, ActorId::zero(), to, value);
        self.emit_event(AdminEvent::Minted { to, value }).expect("event");
        Ok(true)
    }

    // Burn `value` from `from`. Caller must be the admin or hold the minter role.
    #[export(unwrap_result)]
    pub fn burn(&mut self, from: ActorId, value: U256) -> Result<bool, TokenError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_minter(caller)?;
            s.burn(from, value)?;
        }
        emit_vft_transfer(self.state, from, ActorId::zero(), value);
        self.emit_event(AdminEvent::Burned { from, value }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn grant_minter(&mut self, account: ActorId) -> Result<bool, TokenError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.minters.insert(account);
        }
        self.emit_event(AdminEvent::MinterGranted { account }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn revoke_minter(&mut self, account: ActorId) -> Result<bool, TokenError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.minters.remove(&account);
        }
        self.emit_event(AdminEvent::MinterRevoked { account }).expect("event");
        Ok(true)
    }

    // Configure the public faucet. `amount` of zero disables it.
    #[export(unwrap_result)]
    pub fn set_faucet(&mut self, amount: U256, cooldown_secs: u64) -> Result<bool, TokenError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.faucet_amount = amount;
            s.faucet_cooldown_ms = cooldown_secs.saturating_mul(1000);
        }
        self.emit_event(AdminEvent::FaucetConfigured { amount, cooldown_secs }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn transfer_admin(&mut self, to: ActorId) -> Result<bool, TokenError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            if to.is_zero() { return Err(TokenError::ZeroAddress); }
            s.admin = to;
        }
        self.emit_event(AdminEvent::AdminTransferred { from: caller, to }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn pause(&mut self) -> Result<bool, TokenError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.paused = true;
        }
        self.emit_event(AdminEvent::Paused { by: caller }).expect("event");
        Ok(true)
    }

    #[export(unwrap_result)]
    pub fn resume(&mut self) -> Result<bool, TokenError> {
        let caller = Syscall::message_source();
        {
            let mut s = self.state.borrow_mut();
            s.ensure_admin(caller)?;
            s.paused = false;
        }
        self.emit_event(AdminEvent::Resumed { by: caller }).expect("event");
        Ok(true)
    }

    #[export]
    pub fn admin_account(&self) -> ActorId {
        self.state.borrow().admin
    }

    #[export]
    pub fn minters(&self) -> Vec<ActorId> {
        self.state.borrow().minters.iter().copied().collect()
    }

    #[export]
    pub fn is_minter(&self, account: ActorId) -> bool {
        let s = self.state.borrow();
        s.admin == account || s.minters.contains(&account)
    }

    #[export]
    pub fn is_paused(&self) -> bool {
        self.state.borrow().paused
    }
}

// ---------------------------------------------------------------------------
// Faucet service: anyone can claim test balance
// ---------------------------------------------------------------------------

#[sails_rs::sails_type]
#[sails_rs::event]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaucetEvent {
    Claimed { to: ActorId, value: U256, next_claim_at: u64 },
}

pub struct Faucet<'a> {
    state: &'a RefCell<TokenState>,
}

impl<'a> Faucet<'a> {
    pub fn new(state: &'a RefCell<TokenState>) -> Self {
        Self { state }
    }
}

#[sails_rs::service(events = FaucetEvent)]
impl Faucet<'_> {
    // Mint the configured faucet amount to the caller. Returns the amount minted.
    #[export]
    pub fn claim(&mut self) -> Result<U256, TokenError> {
        let to = Syscall::message_source();
        let now = Syscall::block_timestamp();
        let (value, next_claim_at) = {
            let mut s = self.state.borrow_mut();
            let value = s.faucet_claim(to, now)?;
            (value, s.next_claim_at(&to))
        };
        emit_vft_transfer(self.state, ActorId::zero(), to, value);
        self.emit_event(FaucetEvent::Claimed { to, value, next_claim_at }).expect("event");
        Ok(value)
    }

    #[export]
    pub fn config(&self) -> FaucetConfig {
        let s = self.state.borrow();
        FaucetConfig { amount: s.faucet_amount, cooldown_secs: s.faucet_cooldown_ms / 1000 }
    }

    // Unix timestamp (ms) after which `account` may claim again. Zero if it never claimed.
    #[export]
    pub fn next_claim_at(&self, account: ActorId) -> u64 {
        self.state.borrow().next_claim_at(&account)
    }
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

pub struct Program {
    state: RefCell<TokenState>,
}

#[sails_rs::program]
impl Program {
    // Deploy a token. The deployer becomes admin. `faucet_amount` is in base units.
    pub fn new(name: String, symbol: String, decimals: u8, faucet_amount: U256, faucet_cooldown_secs: u64) -> Self {
        let state = TokenState {
            name,
            symbol,
            decimals,
            admin: Syscall::message_source(),
            faucet_amount,
            faucet_cooldown_ms: faucet_cooldown_secs.saturating_mul(1000),
            ..Default::default()
        };
        Self { state: RefCell::new(state) }
    }

    // Keep `vft` first: `VFT_ROUTE_IDX` relies on it being service #1.
    pub fn vft(&self) -> Vft<'_> {
        Vft::new(&self.state)
    }

    pub fn admin(&self) -> Admin<'_> {
        Admin::new(&self.state)
    }

    pub fn faucet(&self) -> Faucet<'_> {
        Faucet::new(&self.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(n: u64) -> ActorId { ActorId::from(n) }
    fn u(n: u64) -> U256 { U256::from(n) }

    fn fresh() -> TokenState {
        TokenState { admin: a(1), faucet_amount: u(1_000), faucet_cooldown_ms: 60_000, ..Default::default() }
    }

    #[test]
    fn mint_transfer_burn_round_trip() {
        let mut s = fresh();
        s.mint(a(2), u(100)).unwrap();
        assert_eq!(s.total_supply, u(100));
        s.transfer(a(2), a(3), u(40)).unwrap();
        assert_eq!(s.balance_of(&a(2)), u(60));
        assert_eq!(s.balance_of(&a(3)), u(40));
        assert_eq!(s.transfer(a(2), a(3), u(61)), Err(TokenError::InsufficientBalance));
        s.burn(a(3), u(40)).unwrap();
        assert_eq!(s.total_supply, u(60));
        assert!(s.balances.get(&a(3)).is_none(), "zero balances are pruned");
    }

    #[test]
    fn allowance_is_spent_and_max_is_infinite() {
        let mut s = fresh();
        s.mint(a(2), u(100)).unwrap();
        s.approve(a(2), a(9), u(30)).unwrap();
        s.spend_allowance(a(2), a(9), u(10)).unwrap();
        assert_eq!(s.allowance(&a(2), &a(9)), u(20));
        assert_eq!(s.spend_allowance(a(2), a(9), u(21)), Err(TokenError::InsufficientAllowance));
        s.approve(a(2), a(9), U256::MAX).unwrap();
        s.spend_allowance(a(2), a(9), u(99)).unwrap();
        assert_eq!(s.allowance(&a(2), &a(9)), U256::MAX);
    }

    #[test]
    fn faucet_respects_cooldown() {
        let mut s = fresh();
        assert_eq!(s.faucet_claim(a(5), 10_000).unwrap(), u(1_000));
        assert_eq!(s.faucet_claim(a(5), 20_000), Err(TokenError::FaucetCooldown { next_claim_at: 70_000 }));
        assert_eq!(s.faucet_claim(a(5), 70_000).unwrap(), u(1_000));
        assert_eq!(s.balance_of(&a(5)), u(2_000));
        s.faucet_amount = U256::zero();
        assert_eq!(s.faucet_claim(a(6), 1), Err(TokenError::FaucetDisabled));
    }

    #[test]
    fn pause_blocks_movement() {
        let mut s = fresh();
        s.mint(a(2), u(100)).unwrap();
        s.paused = true;
        assert_eq!(s.transfer(a(2), a(3), u(1)), Err(TokenError::Paused));
        assert_eq!(s.mint(a(2), u(1)), Err(TokenError::Paused));
        assert_eq!(s.faucet_claim(a(2), 1), Err(TokenError::Paused));
    }

    #[test]
    fn roles() {
        let mut s = fresh();
        assert!(s.ensure_minter(a(1)).is_ok(), "admin can mint");
        assert_eq!(s.ensure_minter(a(7)), Err(TokenError::Unauthorized));
        s.minters.insert(a(7));
        assert!(s.ensure_minter(a(7)).is_ok());
        assert_eq!(s.ensure_admin(a(7)), Err(TokenError::Unauthorized));
    }
}
