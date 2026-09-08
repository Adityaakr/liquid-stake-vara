## `Pool::request_unbond` in programs/vara-pool/app/src/lib.rs (L724-L735)

**Purpose:** Timed exit entry point: burn shares and record an `Unbond` via `PoolState::request_unbond`; no value moves in this message.

**Inputs & Assumptions:**
- `shares` (U256): untrusted.
- Implicit: `message_source` (L729), `block_timestamp` (L728).
- Preconditions: as `PoolState::request_unbond`.

**Outputs & Effects:**
- State writes from `PoolState::request_unbond` (L730); then `VftEvent::Transfer {owner -> 0, shares}` (L732) and `PoolEvent::UnbondRequested {owner, id, shares, assets, claimable_at}` (L733); reply `Ok(entry)`.
- On `?` at L729/L730: normal `Err` reply; state as the callee left it (no writes on its checked error paths; the L424 `Overflow` path is the exception noted in its record).
- No `pay_out`, so no compensation branch; the only trap risk is `.expect("event")` (L732-L733).

**Block-by-Block:**

```rust
// L726-L731
let (owner, entry) = {
    let mut s = self.state.borrow_mut();
    let now = now_ms();
    let owner = s.actor_for(Syscall::message_source(), SessionAction::Unbond, now)?;
    (owner, s.request_unbond(owner, shares, now)?)
};
```
- **What:** Resolve and mutate under one borrow.
- **Assumes:** a session key with `Unbond` permission can start the unbond, but claiming later needs `Claim` permission (L743) and pays the owner (L746).

```rust
// L732-L734
emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
self.emit_event(PoolEvent::UnbondRequested { ... }).expect("event");
Ok(entry)
```
- **What:** Burn event for indexers, then the pool event.
- **Establishes:** the receipt-token supply reported by `Vft::total_supply` (L601-L603) dropped by `shares` at L730 even though the VARA has not left; `unbonding_total` carries the claim.

**Cross-Function Dependencies:**
- Callee `PoolState::actor_for`, `request_unbond` (internal): see records.
- Callee `emit_vft_transfer`, `emit_event`: trap on failure.
- Callers: dispatcher only.
- Shared state: as `PoolState::request_unbond`.
- Invariant couplings: with `unbond_period_ms == 0` (configurable, L466), this plus `claim` in the same block is a fee-free instant exit.

**Open Questions:**
- none beyond the trap question shared with `Pool::stake`.
