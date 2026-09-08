## `Vault::request_unbond` in programs/vault/app/src/lib.rs (L787-L797)

**Purpose:** Synchronous handler for the timed exit: resolve the actor, run `VaultState::request_unbond`, emit the burn and `UnbondRequested` events. Exported as `RequestUnbond(shares) -> Result<Unbond, VaultError>` (IDL L161). No token call, no await, no gas gate.

**Inputs & Assumptions:**
- `shares` (U256): payload. Trust: untrusted.
- Implicit: `message_source()`, `now_ms()` (L790-L791).

**Outputs & Effects:**
- State writes (all inside one `borrow_mut`, L789-L793): those of `VaultState::request_unbond` (L442-L456).
- Events: VFT `Transfer{owner → 0, shares}` (L794), `UnbondRequested{owner, id, shares, assets, claimable_at}` (L795).
- Returns the entry or the error. Because the whole handler is one execution, an `Err` after writes is impossible here (the state function returns before writing on every error path except the unreachable `Overflow` at L450-L451); a panic in `emit_event` (L794-L795 `.expect`) discards the whole execution including the writes, so no half-state.

**Block-by-Block:**

```rust
// L788-L793
let (owner, entry) = {
    let mut s = self.state.borrow_mut();
    let now = now_ms();
    let owner = s.actor_for(Syscall::message_source(), SessionAction::Unbond, now)?;
    (owner, s.request_unbond(owner, shares, now)?)
};
```
- **What:** Actor resolution then the transition.
- **Why here:** One borrow, one block.
- **Establishes:** an unbond entry priced at `now`, shares burnt, `unbonding_total` raised.
- **Depended on by:** L794-L796; `claim` later.

```rust
// L794-L796
emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
self.emit_event(VaultEvent::UnbondRequested { ... }).expect("event");
Ok(entry)
```
- **What:** Events and reply.
- **Assumes:** `emit_event` succeeds (trap otherwise; atomic with the writes since there is no await).

**Cross-Function Dependencies:**
- Callees: `actor_for`, `VaultState::request_unbond`, `emit_vft_transfer`.
- Callers: any actor; a session key with `Unbond`.
- Shared state: as `VaultState::request_unbond`.
- Invariant couplings: this is the only exit path without an instant fee and without an outbound transfer; with `unbond_period_secs == 0` (allowed by L221/L508) an unbond is claimable in the same block, so `request_unbond` + `claim` is a fee-free instant exit whenever the admin configures a zero period.

**Open Questions:**
- none.
