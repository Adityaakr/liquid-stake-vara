## `Pool::claim` in programs/vara-pool/app/src/lib.rs (L738-L756)

**Purpose:** Pay out a matured unbond: `take_unbond` releases the counters, `pay_out` sends the VARA to the owner, `restore_unbond` compensates a refused send.

**Inputs & Assumptions:**
- `id` (u64): untrusted.
- Implicit: `message_source` (L743), `block_timestamp` (L742).
- Preconditions: as `PoolState::take_unbond` (ownership by list, maturity, `assets <= reserve`, not paused).

**Outputs & Effects:**
- Order: counters released (L744) -> borrow dropped (L745) -> `pay_out(owner, entry.assets)` (L746) -> `Ok`: `PoolEvent::Claimed` (L748), reply `Ok(assets)`; `Err`: `restore_unbond` (L752), reply `Err(TransferFailed)`.
- No `Vft` event here (shares were burned at request time).

**Block-by-Block:**

```rust
// L740-L745
let (owner, entry) = {
    let mut s = self.state.borrow_mut();
    let now = now_ms();
    let owner = s.actor_for(Syscall::message_source(), SessionAction::Claim, now)?;
    (owner, s.take_unbond(owner, id, now)?)
};
```
- **What:** Resolve, then remove the entry and release `reserve`/`unbonding_total`.
- **Assumes:** the resolved `owner` is the one whose list holds `id` (L434-L435); a session key claims only its owner's entries and the VARA goes to the owner (L746).

```rust
// L746-L755
match pay_out(owner, entry.assets) {
    Ok(()) => { self.emit_event(PoolEvent::Claimed { owner, id, assets: entry.assets }).expect("event"); Ok(entry.assets) }
    Err(e) => { self.state.borrow_mut().restore_unbond(owner, entry); Err(e) }
}
```
- **What:** Same pay/compensate shape as `unstake`.
- **Assumes:** the same things about `send_bytes_with_gas` as `Pool::unstake` (queued not delivered; ED and balance errors surface synchronously; 0-gas message to a program destination is runtime-defined).
- **Establishes:** on `Err` the entry is back with the same id and the counters restored (see `restore_unbond`), so the user can retry.

**Cross-Function Dependencies:**
- Callee `PoolState::actor_for`, `take_unbond`, `restore_unbond` (internal).
- Callee `pay_out` -> `send_bytes_with_gas` (external-source-available).
- Callee `emit_event` (trap on failure, after the payout is queued).
- Callers: dispatcher only.
- Shared state: `unbonds`, `unbonding_total`, `reserve`.
- Invariant couplings: `take_unbond` checks `reserve` only (L437); the payout draws from the same program balance as `unstake` and `collect_fees`.

**Open Questions:**
- same two runtime questions as `Pool::unstake`.
