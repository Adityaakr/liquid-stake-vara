## `Vft::transfer_from` in programs/demo-token/app/src/lib.rs (L204-L214)

**Purpose:** Exported VFT `TransferFrom(from, to, value) -> bool throws TokenError` (IDL L28). The vault's `pull_underlying` (vault lib.rs L713-L722) calls it with `from = depositor`, `to = vault program id`, `value = assets` for `deposit` (vault L747) and `fund_rewards` (vault L826-L834, open to anyone); the vault is the `spender`.

**Inputs & Assumptions:**
- `from` (ActorId): payer. Trust: untrusted. Authorization comes from `spend_allowance` (L209) or from `spender == from`.
- `to` (ActorId), `value` (U256). Trust: untrusted; validated in `transfer` (L99-L100).
- Implicit: `spender = Syscall::message_source()` (L206). `allowances`, `balances`, `paused`.
- Preconditions: for `spender != from`, `allowance(from, spender) >= value` or `== U256::MAX` — established by `spend_allowance` L113-L115.

**Outputs & Effects:**
- Success: allowance reduced unless MAX or self-spend (L209 -> L114-L116), balances moved (L210), `Transfer { from, to, value }` emitted (L212), reply `true` (L213). No `Approval` event is emitted for the allowance decrement.
- Errors, via `unwrap_result` structured panic (L204; exposure.rs L97-L104; macros.rs L307-L325):
  - `InsufficientAllowance` (L115): no writes.
  - `Paused` (L98), `ZeroAddress` (L99), `ZeroAmount` (L100), `InsufficientBalance` (L101): reached after `spend_allowance` may already have written the reduced allowance at L116. The write sits in the in-memory `RefCell`; the handler then panics.

**Block-by-Block:**

```rust
// L206-L211
let spender = Syscall::message_source();
{
    let mut s = self.state.borrow_mut();
    if spender != from { s.spend_allowance(from, spender, value)?; }
    s.transfer(from, to, value)?;
}
```
- **What:** allowance check-and-decrement, then balance move, under one borrow.
- **Why here:** `spend_allowance` first means the authorization is settled before balances move; the `spender == from` branch lets an owner move its own funds with no allowance, so `approve(self, ...)` is never required or consulted.
- **Assumes:** the two writes (L116 then L101-L102) either both persist or neither does. Established by: the Gear runtime discarding a trapped message's state changes, which is outside this repository. Nothing in the token guards against a persisted allowance decrement followed by a failed transfer. No test exercises a write-then-fail path (tests/gtest.rs L101-L107 fail at L115 before any write; the pause test at L126-L129 uses `mint`, which fails before writing).
- **Establishes:** on the success path, `allowance' == allowance - value` (or unchanged for MAX / self), balances moved, sum(balances) unchanged.
- **Depended on by:** the vault's `Ok(Ok(true))` arm (vault L717).

```rust
// L212-L213
self.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
Ok(true)
```
- **What:** event and fixed `true` reply.
- **Why here:** after the borrow block, so emitting cannot conflict with the `RefMut`.
- **Assumes:** emit succeeds; on failure the `expect` panics and the completed writes ride on the same atomicity assumption as above.
- **Establishes:** one `Transfer` event per successful pull.
- **Depended on by:** nothing in the vault.

**What the vault relies on (vault lib.rs L714-L722):**
- Return value: `true` on every success path (L213). `Ok(Ok(false))` (vault L718) is dead with this token.
- Error surface: every `TokenError` arrives as `Ok(Err(e))` because the client decodes the panic payload (client/mod.rs L450-L464) — including `Paused`, `InsufficientAllowance`, `InsufficientBalance`, `ZeroAmount`. Transport errors (`Err(e)`, vault L720) cover gas exhaustion inside `TOKEN_CALL_GAS` (vault L46), an unavailable program, or an undecodable payload (a panic payload over `MAX_PANIC_PAYLOAD_SIZE` 1024 bytes falls back to a text panic, macros.rs L317-L322; `TokenError` encodes in a few bytes so that branch is not reached).
- Pause: the vault performs no pause check of its own; a paused underlying surfaces as `TokenRejected` and `deposit` fails before `settle_deposit` (vault L747 precedes L748-L753).
- Zero amount: the vault rejects zero earlier (`check_deposit`, vault L380) so the token's `ZeroAmount` is not reached from `deposit`.
- Allowance: the depositor must have approved the vault program id as spender; the vault does not read `allowance` first.
- Re-entry: none — the token makes no outbound call during the handler and has no `.await`.

**Cross-Function Dependencies:**
- Callee `spend_allowance` (internal, L112-L118): establishes authorization only when `spender != from`; MAX allowance is never decremented; no pause check.
- Callee `TokenState::transfer` (internal, L97-L103): establishes pause, zero and balance checks; errors after `spend_allowance` has written.
- Callee `emit_event` (external-source-available, events.rs L92-L101).
- Callers: any actor; vault `pull_underlying` L716.
- Shared state: `allowances` (`approve`), `balances` (`mint`, `burn`, `transfer`), `paused`.
- Invariant couplings: with a MAX allowance, one approval authorizes unbounded cumulative pulls by the vault program; the token has no per-spender cap beyond the owner's balance (L101).

**Open Questions:**
- unclear; need to confirm against the Gear runtime (not in repo) that a `Syscall::panic` / `expect` trap in a Sails handler discards the `RefCell` writes made earlier in the same message. Every "no partial write" claim for the `Paused`/`InsufficientBalance`-after-decrement path and for emit failures rests on it.
