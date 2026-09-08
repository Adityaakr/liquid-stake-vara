## `Pool::stake` in programs/vara-pool/app/src/lib.rs (L680-L697)

**Purpose:** Entry point for deposits. The VARA attached to the message is the deposit; on any error the same value is sent back with the reply (comment L678-L679, L695).

**Inputs & Assumptions:**
- No parameters. Implicit: `Syscall::message_value()` (L682; gcore `msg::value`, syscalls.rs L34-L36), `Syscall::message_source()` (L687), `block_timestamp` via `now_ms()` (L686).
- Trust: any actor may call; identity resolved by `actor_for` (L687).
- Preconditions:
  - The attached value is in the program's balance when the handler runs: Gear delivers value with the message; nothing in-file verifies it.
  - The `Vft` service is route 1 (L24, L962-L964): `emit_vft_transfer` (L621-L625) hard-codes `VFT_ROUTE_IDX`; the sails program macro numbers services in declaration order starting at 1 (sails-macros-core program/mod.rs L167-L182, L695-L700), and `vft()` is declared before `pool()` (L963, L967).

**Outputs & Effects:**
- On `Ok`: state changes from `PoolState::stake` persist; emits `VftEvent::Transfer {0 -> owner, shares}` (L691) and `PoolEvent::Staked {owner, assets, shares, rate}` (L692) with `rate` computed *after* the stake (L687); replies `Ok(shares)` with no value (L693).
- On `Err`: no `PoolState` write except the `sweep_orphans` case (see its record); replies `Err(e)` carrying `value` back (L695; sails exposure.rs L109-L113, macros.rs L293 `reply_bytes(encoded, value)`).
- Events are sent as messages to `ActorId::zero()` (sails events.rs L92-L102) and `.expect("event")` traps on failure (L691-L692). A trap discards the execution's state writes and (as far as this file is concerned) its reply; the value's fate on a trap is runtime behaviour, see open questions.

**Block-by-Block:**

```rust
// L682-L688
let value = Syscall::message_value();
let assets = U256::from(value);
let outcome = {
    let mut s = self.state.borrow_mut();
    let now = now_ms();
    s.actor_for(Syscall::message_source(), SessionAction::Stake, now).and_then(|owner| s.stake(owner, assets, now).map(|shares| (owner, shares, s.rate_at(now))))
};
```
- **What:** Resolve the actor, stake, snapshot the post-stake rate; all under one `RefCell` borrow that ends at L688.
- **Why here:** The borrow is released before `emit_event`, which is a separate `&mut self` call on the service, not on the state (L692).
- **Assumes:** `actor_for` returning `Ok(owner)` where `owner != sender` means the *key's* VARA buys shares for the *owner* (L687); a refund on `Err` goes to the reply's recipient, i.e. the sender/key (L695).

```rust
// L689-L696
match outcome {
    Ok((owner, shares, rate)) => { emit_vft_transfer(...); self.emit_event(PoolEvent::Staked {...}).expect("event"); CommandReply::new(Ok(shares)) }
    Err(e) => CommandReply::new(Err(e)).with_value(value),
}
```
- **What:** Success keeps the value and emits; failure returns it.
- **Assumes:** `reply_bytes(encoded, value)` (macros.rs L293) succeeds for the refund. gcore documents `InsufficientValue` (gear-core-errors L129-L132) for `0 < value < existential deposit`; if the refund reply hits that, the macro's `expect("Failed to send output")` traps.
- **Establishes:** `reserve` grows by exactly `value` on success (L365) and the program keeps `value`; the two agree by construction (L683).

**Cross-Function Dependencies:**
- Callee `PoolState::actor_for` (internal): identity; see record.
- Callee `PoolState::stake` (internal): all checks and writes; see record.
- Callee `PoolState::rate_at` (internal): event field only.
- Callee `emit_vft_transfer` (internal, L621-L625) -> `gstd::msg::send_bytes(zero, payload, 0)` (external-source-available, sails events.rs L99): can fail on message-count/size limits; failure traps via `expect`.
- Callee `Syscall::message_value` / `message_source` / `block_timestamp` (external-source-available, syscalls.rs L30-L36, L64-L66): thin wrappers over gcore; trusted runtime values.
- Callers: the sails dispatcher (generated). No in-file caller.
- Shared state: everything `PoolState::stake` touches.
- Invariant couplings: this is one of two places (with `fund_rewards`) where real value enters `reserve` accounting; VARA sent to the program any other way is invisible to `reserve` (comment L166-L167).

**Open Questions:**
- unclear; need to inspect Gear core-processor trap handling to state what happens to the attached value and to already-queued outgoing messages when a handler traps after `send_bytes` (the `.expect("event")` paths at L691-L692).
- unclear; whether a refund reply carrying `value` below the chain's existential deposit is accepted (`MIN_STAKE` is 0.01 VARA, L29-L30; a stake of e.g. 0.005 VARA is refunded at L695).
