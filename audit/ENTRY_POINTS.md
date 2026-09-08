# Entry Point Analysis: Vale Protocol (liquid-stake) Sails programs

**Analyzed**: 2026-09-08
**Scope**: `programs/vara-pool/app/src/lib.rs`, `programs/vault/app/src/lib.rs`, `programs/demo-token/app/src/lib.rs`
**Languages**: Rust / Gear-Vara Sails (`sails_rs` 1.x, `#![no_std]` wasm programs)
**Focus**: State-changing functions only (view/pure excluded)
**Tooling**: Slither not applicable (no Solidity); manual analysis.

## How entry points work in these programs

- A `#[sails_rs::program]` `impl Program` exposes one constructor (`new`) and one accessor per service. Constructors run exactly once, at program initialization; the uploader/initializer is `Syscall::message_source()` and becomes `admin`.
- Every `#[export]` method on a `#[sails_rs::service]` impl is externally callable by any actor (EOA or program) that sends a message to the program. `&mut self` exports are commands (state-changing) and are the subject of this report; `&self` exports are queries and are excluded.
- `#[export(unwrap_result)]` means an `Err` panics the message: the whole message execution is rolled back and the reply is an error reply. Plain `#[export]` returning `Result` sends `Err` as a normal reply, and any state written before the `Err` persists (the code compensates by hand where needed).
- Access control is entirely manual. The patterns in use are:
  - `ensure_admin(caller)` — `caller == state.admin` (single admin, transferable via `transfer_admin`).
  - `ensure_minter(caller)` — `caller == admin || minters.contains(caller)` (demo-token only).
  - `actor_for(sender, action, now)` — session-key redirection (pool and vault): if `sender` is an accepted session key, the command acts for the session's *owner* (balances, unbonds and payouts are the owner's), provided the session has not expired and `action` is in its allow-list. Otherwise the command acts for `sender` itself.
  - Direct `Syscall::message_source()` identity — the caller acts for itself only (Vft transfers, session management, `fund_rewards`, faucet).
- Value: any Gear message may carry native VARA. Only the two pool commands marked **payable** below read `Syscall::message_value()`; they refund it on error via `CommandReply::with_value`. No other command reads attached value.
- Async: only the vault has cross-program calls. Async commands await a call into the underlying token (`DemoTokenProgram::client(underlying).vft().transfer_from / transfer`, gas limit `TOKEN_CALL_GAS`), and start with `ensure_gas(GAS_ONE_CALL)`. State is persisted at each `.await`, so other messages can execute between the pre-await and post-await halves.

## Summary

| Category | Count |
|----------|-------|
| Public (Unrestricted) | 26 |
| Role-Restricted (admin) | 16 |
| Role-Restricted (minter or admin) | 2 |
| Restricted (Review Required) | 0 |
| Contract-Only | 0 |
| **Total state-changing entry points** | **44** |
| Initializers (constructors, one-shot at deploy) | 3 |

Per program: vara-pool 16 (11 public, 5 admin), vault 16 (11 public, 5 admin), demo-token 12 (4 public, 6 admin, 2 minter-or-admin).

Property tags used below: **payable** = reads `message_value()`; **async** = awaits a cross-program call; **session** = caller may be an accepted session key acting for another account via `actor_for`.

---

## Public Entry Points (Unrestricted)

State-changing functions callable by anyone. Prioritize for attack-surface analysis.

### vara-pool (`programs/vara-pool/app/src/lib.rs`) — service `Vft` (route 1)

| Function | File | Notes |
|----------|------|-------|
| `approve(spender, value)` | `programs/vara-pool/app/src/lib.rs:563` | `unwrap_result`. Owner = `message_source()`. Blocked when paused. Not session-redirectable. |
| `transfer(to, value)` | `programs/vara-pool/app/src/lib.rs:571` | `unwrap_result`. From = `message_source()`. Blocked when paused. Not session-redirectable. |
| `transfer_from(from, to, value)` | `programs/vara-pool/app/src/lib.rs:579` | `unwrap_result`. Spender = `message_source()`; allowance skipped when `spender == from`; `U256::MAX` allowance is never decremented. Blocked when paused. |

### vara-pool — service `Pool` (route 2)

| Function | File | Notes |
|----------|------|-------|
| `stake()` | `programs/vara-pool/app/src/lib.rs:681` | **payable**, **session** (`SessionAction::Stake`). Deposit = attached VARA; mints kVARA to the resolved owner. Refunds value on `Err`. Blocked when paused; `MIN_STAKE` floor; sweeps orphaned distributable into fees when `total_shares == 0`. |
| `unstake(shares)` | `programs/vara-pool/app/src/lib.rs:701` | **session** (`Unstake`). Burns shares, accrues instant fee, sends `net` VARA to the *owner* via `send_bytes_with_gas(.., 0, value)`; reverts the accounting on send failure. Blocked when paused. |
| `request_unbond(shares)` | `programs/vara-pool/app/src/lib.rs:725` | **session** (`Unbond`). Burns shares now, locks assets in `unbonding_total`, records an `Unbond` entry for the owner. Blocked when paused. |
| `claim(id)` | `programs/vara-pool/app/src/lib.rs:739` | **session** (`Claim`). Pays a matured unbond entry to the owner; restores the entry on send failure. Blocked when paused. |
| `fund_rewards()` | `programs/vara-pool/app/src/lib.rs:761` | **payable**. Anyone may fund; attached VARA joins the vesting tranche (amount-weighted re-timing). Refunds value on `Err`. Not gated by `paused`. Not session-redirectable (`from = message_source()`). |
| `create_session(key, duration_secs, actions)` | `programs/vara-pool/app/src/lib.rs:863` | Owner = `message_source()`. Writes `pending_sessions[owner]`; no effect on `key` until accepted. Rejects `key == owner`, zero key, empty actions, duration 0 or > 30 days, or a key already bound to a different owner. |
| `accept_session(owner)` | `programs/vara-pool/app/src/lib.rs:873` | Caller must equal `pending_sessions[owner].key` (data-dependent check, not a privileged role). Activates the session, replaces the owner's previous session and its key mapping. |
| `revoke_session()` | `programs/vara-pool/app/src/lib.rs:882` | Owner = `message_source()`. Drops the caller's pending and active session. Not gated by `paused`. Only the owner can revoke; a key cannot revoke itself. |

### vault (`programs/vault/app/src/lib.rs`) — service `Vft` (route 1)

| Function | File | Notes |
|----------|------|-------|
| `approve(spender, value)` | `programs/vault/app/src/lib.rs:606` | `unwrap_result`. Same shape as pool. |
| `transfer(to, value)` | `programs/vault/app/src/lib.rs:614` | `unwrap_result`. Same shape as pool. |
| `transfer_from(from, to, value)` | `programs/vault/app/src/lib.rs:622` | `unwrap_result`. Same shape as pool. |

### vault — service `Vault` (route 2)

| Function | File | Notes |
|----------|------|-------|
| `deposit(assets)` | `programs/vault/app/src/lib.rs:738` | **async**, **session** (`Deposit`). `ensure_gas` first. Pre-await: `check_deposit` (paused, `MIN_DEPOSIT`, non-zero shares) under a shared borrow. Awaits `underlying.vft.transfer_from(owner, vault, assets)` — tokens are pulled from the *resolved owner* (owner must have approved the vault). Post-await: `settle_deposit` re-prices shares at the post-await block and mints `max(shares, 1)`. |
| `redeem(shares)` | `programs/vault/app/src/lib.rs:761` | **async**, **session** (`Redeem`). Pre-await: burns shares, accrues fee, reduces `holdings` (`begin_redeem`), emits Vft burn. Awaits `underlying.vft.transfer(owner, net)`; on failure `revert_redeem` and re-emits a mint event. |
| `request_unbond(shares)` | `programs/vault/app/src/lib.rs:787` | **session** (`Unbond`). Synchronous. Same shape as pool. |
| `claim(id)` | `programs/vault/app/src/lib.rs:801` | **async**, **session** (`Claim`). Pre-await `take_unbond`; awaits `underlying.vft.transfer(owner, assets)`; `restore_unbond` on failure. |
| `fund_rewards(assets)` | `programs/vault/app/src/lib.rs:826` | **async**. Anyone may fund. Pulls `assets` from `message_source()` (not session-redirected) via `transfer_from`, then `fund` re-times the vesting tranche. Not gated by `paused`. |
| `create_session(key, duration_secs, actions)` | `programs/vault/app/src/lib.rs:924` | Same as pool. |
| `accept_session(owner)` | `programs/vault/app/src/lib.rs:934` | Same as pool. |
| `revoke_session()` | `programs/vault/app/src/lib.rs:943` | Same as pool. |

### demo-token (`programs/demo-token/app/src/lib.rs`) — service `Vft` (route 1)

| Function | File | Notes |
|----------|------|-------|
| `approve(spender, value)` | `programs/demo-token/app/src/lib.rs:189` | `unwrap_result`. Blocked when paused. |
| `transfer(to, value)` | `programs/demo-token/app/src/lib.rs:197` | `unwrap_result`. Blocked when paused. Called cross-program by the vault (vault is `message_source()`) in `redeem`, `claim`, `collect_fees`. |
| `transfer_from(from, to, value)` | `programs/demo-token/app/src/lib.rs:205` | `unwrap_result`. Allowance skipped when `spender == from`; `U256::MAX` is infinite. Called cross-program by the vault as spender in `deposit` and `fund_rewards`. |

### demo-token — service `Faucet` (route 3)

| Function | File | Notes |
|----------|------|-------|
| `claim()` | `programs/demo-token/app/src/lib.rs:436` | Mints `faucet_amount` to `message_source()`, subject to per-account cooldown (`last_claim + faucet_cooldown_ms`). Disabled when `faucet_amount == 0`. Blocked when paused. Increases `total_supply`. |

---

## Role-Restricted Entry Points

### Admin / Owner

Restriction is `ensure_admin(caller)`: `caller == state.admin`. Admin is set to the deployer in the constructor and can be moved with `transfer_admin` (single-step, no acceptance; zero address rejected).

#### vara-pool — service `Pool`

| Function | File | Restriction |
|----------|------|-------------|
| `set_config(instant_fee_bps, unbond_period_secs, vesting_period_secs)` | `programs/vara-pool/app/src/lib.rs:781` | `ensure_admin`. Fee <= 10%, vesting in [1h, 1y]; unbond period unbounded. |
| `pause()` | `programs/vara-pool/app/src/lib.rs:793` | `ensure_admin`. Blocks stake/unstake/unbond/claim/Vft transfers and approvals. Does not block `fund_rewards`, session management, or admin ops. |
| `resume()` | `programs/vara-pool/app/src/lib.rs:805` | `ensure_admin`. |
| `transfer_admin(to)` | `programs/vara-pool/app/src/lib.rs:817` | `ensure_admin`. Immediate. |
| `collect_fees(to)` | `programs/vara-pool/app/src/lib.rs:831` | `ensure_admin`. Sends the whole `fees_accrued` (instant-exit fees + swept orphans) from `reserve` to an arbitrary `to`; restores on send failure. Not gated by `paused`. |

#### vault — service `Vault`

| Function | File | Restriction |
|----------|------|-------------|
| `set_config(instant_fee_bps, unbond_period_secs, vesting_period_secs)` | `programs/vault/app/src/lib.rs:847` | `ensure_admin`. Same bounds as pool. |
| `pause()` | `programs/vault/app/src/lib.rs:859` | `ensure_admin`. Same coverage as pool. |
| `resume()` | `programs/vault/app/src/lib.rs:871` | `ensure_admin`. |
| `transfer_admin(to)` | `programs/vault/app/src/lib.rs:883` | `ensure_admin`. Immediate. |
| `collect_fees(to)` | `programs/vault/app/src/lib.rs:897` | `ensure_admin`, **async**. `ensure_gas`, then `take_fees` pre-await, awaits `underlying.vft.transfer(to, amount)`, `restore_fees` on failure. Not gated by `paused`. |

#### demo-token — service `Admin` (route 2)

| Function | File | Restriction |
|----------|------|-------------|
| `grant_minter(account)` | `programs/demo-token/app/src/lib.rs:315` | `ensure_admin`, `unwrap_result`. Adds to `minters` set; no zero-address check. |
| `revoke_minter(account)` | `programs/demo-token/app/src/lib.rs:327` | `ensure_admin`, `unwrap_result`. |
| `set_faucet(amount, cooldown_secs)` | `programs/demo-token/app/src/lib.rs:340` | `ensure_admin`, `unwrap_result`. No bounds on amount or cooldown; `amount == 0` disables the faucet. |
| `transfer_admin(to)` | `programs/demo-token/app/src/lib.rs:353` | `ensure_admin`, `unwrap_result`. Immediate. |
| `pause()` | `programs/demo-token/app/src/lib.rs:366` | `ensure_admin`, `unwrap_result`. Blocks transfer/approve/mint/burn/faucet. |
| `resume()` | `programs/demo-token/app/src/lib.rs:378` | `ensure_admin`, `unwrap_result`. |

### Other Roles

| Function | File | Restriction | Role |
|----------|------|-------------|------|
| `mint(to, value)` | `programs/demo-token/app/src/lib.rs:288` | `ensure_minter(caller)`: `caller == admin || minters.contains(caller)`; `unwrap_result`; blocked when paused | minter (or admin) |
| `burn(from, value)` | `programs/demo-token/app/src/lib.rs:302` | `ensure_minter(caller)`; `unwrap_result`; blocked when paused. Burns from an arbitrary `from` with no allowance check. | minter (or admin) |

---

## Restricted (Review Required)

None. Every restricted command uses one of the two explicit checks above (`ensure_admin`, `ensure_minter`). The one data-dependent caller check outside those is `accept_session` (caller must be the key named in the owner's pending proposal); it is listed under Public because it does not confer a privileged role, only binds the caller as a delegate of `owner`.

---

## Session-Key Delegation (cross-cutting)

Commands that resolve the acting account through `actor_for` (`programs/vara-pool/app/src/lib.rs:474`, `programs/vault/app/src/lib.rs:516`). A session key can drive these on behalf of its owner; all effects (share balances, unbond entries, payouts, token pulls) land on the owner, never on the key.

| Program | Command | Required `SessionAction` |
|---------|---------|--------------------------|
| vara-pool | `stake` (`lib.rs:681`) | `Stake` |
| vara-pool | `unstake` (`lib.rs:701`) | `Unstake` |
| vara-pool | `request_unbond` (`lib.rs:725`) | `Unbond` |
| vara-pool | `claim` (`lib.rs:739`) | `Claim` |
| vault | `deposit` (`lib.rs:738`) | `Deposit` |
| vault | `redeem` (`lib.rs:761`) | `Redeem` |
| vault | `request_unbond` (`lib.rs:787`) | `Unbond` |
| vault | `claim` (`lib.rs:801`) | `Claim` |

Session lifecycle: `create_session` (owner proposes) -> `accept_session` (key accepts; replaces any prior active session for that owner) -> `revoke_session` (owner drops). Expiry is checked at use time (`now_ms >= expires_at` -> `SessionExpired`); expired sessions are not garbage-collected and the key stays in `session_keys` until revoked or replaced, but `actor_for` refuses it. One key serves at most one owner (`session_keys` is key -> owner). Note that a bound session key that calls a session-redirected command is *always* redirected to the owner while the session is active: it cannot act for its own balance through those commands until the session expires or is revoked (Vft `transfer`/`approve`, `fund_rewards` and session management still act on the key itself).

Not session-redirectable: all `Vft` commands, `fund_rewards`, `create_session`/`accept_session`/`revoke_session`, and all admin commands.

## Payable and Async (cross-cutting)

| Program | Command | Payable (`message_value`) | Async (cross-program) |
|---------|---------|---------------------------|-----------------------|
| vara-pool | `stake` (`lib.rs:681`) | yes; refunded on `Err` | no |
| vara-pool | `fund_rewards` (`lib.rs:761`) | yes; refunded on `Err` | no |
| vara-pool | `unstake`, `claim`, `collect_fees` | no (they *send* value out via `pay_out`, `lib.rs:672`) | no |
| vault | `deposit` (`lib.rs:738`) | no | yes: `underlying.vft.transfer_from` |
| vault | `redeem` (`lib.rs:761`) | no | yes: `underlying.vft.transfer` |
| vault | `claim` (`lib.rs:801`) | no | yes: `underlying.vft.transfer` |
| vault | `fund_rewards` (`lib.rs:826`) | no | yes: `underlying.vft.transfer_from` |
| vault | `collect_fees` (`lib.rs:897`) | no | yes: `underlying.vft.transfer` |

Native value attached to any command other than pool `stake`/`fund_rewards` is not read by the program; it lands on the program's account balance and is not reflected in `reserve`/`holdings` accounting. The vault program never reads or sends native value.

Every vault async command begins with `ensure_gas(GAS_ONE_CALL)` (`lib.rs:709`) and each token call is capped at `TOKEN_CALL_GAS`. The `underlying` token address is fixed in the constructor; there is no setter.

---

## Contract-Only (Internal Integration Points)

None by construction: no command checks that the caller is a program. The integration edges are ordinary public entry points invoked program-to-program:

| Function | File | Expected Caller |
|----------|------|-----------------|
| demo-token `Vft.transfer_from(from, to, value)` | `programs/demo-token/app/src/lib.rs:205` | vault program as spender (`deposit`, `fund_rewards`); relies on a prior `approve(vault, ..)` by the user |
| demo-token `Vft.transfer(to, value)` | `programs/demo-token/app/src/lib.rs:197` | vault program as `from` (`redeem`, `claim`, `collect_fees`) |
| demo-token `Admin.mint` / `Admin.burn` | `programs/demo-token/app/src/lib.rs:288`, `:302` | any account in `minters` (the module docs say "so the vault can mint yield"); the vault source contains no call to these |

---

## Initializers

| Function | File | Notes |
|----------|------|-------|
| `Program::new(name, symbol, decimals, instant_fee_bps, unbond_period_secs, vesting_period_secs, initial_rate)` | `programs/vara-pool/app/src/lib.rs:957` | Runs once at init. `admin = message_source()`. `initial_rate < 1e18` clamped to 1e18; vesting clamped to [1h, 1y]; `instant_fee_bps` is *not* validated here (only in `set_config`); `unbond_period_secs` unbounded. |
| `Program::new(underlying, name, symbol, decimals, instant_fee_bps, unbond_period_secs, vesting_period_secs, initial_rate)` | `programs/vault/app/src/lib.rs:1018` | Same as pool plus the fixed `underlying` token id (no zero check). |
| `Program::new(name, symbol, decimals, faucet_amount, faucet_cooldown_secs)` | `programs/demo-token/app/src/lib.rs:473` | Runs once at init. `admin = message_source()`; `minters` empty; faucet enabled iff `faucet_amount > 0`. |

---

## Files Analyzed

- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs` (16 state-changing entry points + 1 constructor; services `Vft`, `Pool`)
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs` (16 state-changing entry points + 1 constructor; services `Vft`, `Vault`; cross-program client in `token_client` module)
- `/Users/adityakrx/liquid-stake/programs/demo-token/app/src/lib.rs` (12 state-changing entry points + 1 constructor; services `Vft`, `Admin`, `Faucet`)

Queries excluded (`&self` exports): pool/vault `Vft` {allowance, balance_of, total_supply, name, symbol, decimals}; pool/vault {session, pending_session, session_owner, info, rate, preview_stake|preview_deposit, preview_unstake|preview_redeem, position, unbonds}; demo-token `Admin` {admin_account, minters, is_minter, is_paused}, `Faucet` {config, next_claim_at}.

## Analysis Warnings

None. All three files parsed in full; `programs/vault/app/src/lib.rs` lines 1033-1295 are the `#[cfg(test)]` module only.
