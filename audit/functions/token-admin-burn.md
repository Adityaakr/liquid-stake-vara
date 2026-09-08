## `Admin::burn` in programs/demo-token/app/src/lib.rs (L300-L312)

**Purpose:** Exported `Burn(from, value) -> bool throws TokenError` (IDL L79). Privileged supply destruction from an arbitrary account.

**Inputs & Assumptions:**
- `from` (ActorId): the account burned. Trust: untrusted argument. Not compared to the caller; no allowance read (L302-L307, L130-L136).
- `value` (U256). Trust: untrusted. Zero rejected (L132).
- Implicit: `caller = Syscall::message_source()` (L303); `admin`, `minters`, `paused`, `balances`, `total_supply`.
- Preconditions: caller is admin or minter — `ensure_minter` L306.

**Outputs & Effects:**
- Success: `balances[from] -= value` (L133), `total_supply -= value` (L134); VFT `Transfer { from, 0, value }` (L309); `Burned { from, value }` (L310); reply `true` (L311).
- Errors: `Unauthorized` (L306), `Paused` (L131), `ZeroAmount` (L132), `InsufficientBalance` (L133) — no writes; `Overflow` (L134) after the debit (unreachable under the supply invariant). Structured panic via `unwrap_result` (L301).

**Block-by-Block:**

```rust
// L304-L308
{
    let mut s = self.state.borrow_mut();
    s.ensure_minter(caller)?;
    s.burn(from, value)?;
}
```
- **What:** authorize then burn.
- **Why here:** same shape as `mint`.
- **Assumes:** the minter role is the right authority to remove tokens from third parties. Established by: nothing narrower — the function has no owner-consent path.
- **Establishes:** supply invariant preserved.
- **Depended on by:** events at L309-L310.

```rust
// L309-L311
emit_vft_transfer(self.state, from, ActorId::zero(), value);
self.emit_event(AdminEvent::Burned { from, value }).expect("event");
Ok(true)
```
- **What:** two events, fixed `true`.
- **Why here:** after the borrow block.
- **Assumes:** as for `Admin::mint`.
- **Establishes:** burn observable as VFT `Transfer` to zero.

**Cross-Function Dependencies:**
- Callee `ensure_minter` (internal, L159-L161); `TokenState::burn` (internal, L130-L136); `emit_vft_transfer` (internal, L247-L252); `emit_event` (external-source-available).
- Callers: admin or minters. No test in tests/gtest.rs exercises `burn`; the unit test at L520 covers the state function only.
- Shared state: `balances`, `total_supply`.
- Invariant couplings: `from` may be the vault program. The vault never reads its own underlying balance (vault lib.rs L714-L732), so a burn from the vault's address changes what `Vft::transfer` can pay out (L101) without the vault's bookkeeping noticing.

**Open Questions:**
- None.
