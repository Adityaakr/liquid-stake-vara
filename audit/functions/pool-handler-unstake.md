## `Pool::unstake` in programs/vara-pool/app/src/lib.rs (L700-L721)

**Purpose:** Instant exit entry point: burn shares via `PoolState::unstake`, then send the net VARA to the owner with `pay_out`; on a refused send, roll the state back with `revert_unstake`.

**Inputs & Assumptions:**
- `shares` (U256): untrusted.
- Implicit: `message_source` (L705), `block_timestamp` (L704). The message's own attached value is not read; any value attached is kept by the program and not counted in `reserve`.
- Preconditions: as `PoolState::unstake` (balance, pause, reserve cover, fee bound).

**Outputs & Effects:**
- Order: state writes (L707) -> borrow released (L709) -> `pay_out` (L710) -> on `Ok`: events (L712-L713), reply `Ok(preview)`; on `Err`: `revert_unstake` (L717), reply `Err(TransferFailed)`.
- `rate` in the `Unstaked` event is the *pre-exit* rate (L706, before L707).
- `.expect("event")` at L712-L713 traps after the payout message has been queued.

**Block-by-Block:**

```rust
// L702-L709
let (owner, preview, rate) = {
    let mut s = self.state.borrow_mut();
    let now = now_ms();
    let owner = s.actor_for(Syscall::message_source(), SessionAction::Unstake, now)?;
    let rate = s.rate_at(now);
    let p = s.unstake(owner, shares, now)?;
    (owner, p, rate)
};
```
- **What:** Resolve, snapshot rate, mutate. The `?` at L705/L707 returns the error as a normal reply with the state as `PoolState::unstake` left it (no writes on its error paths).
- **Why here:** The borrow must end before `pay_out`, not for re-entrancy (there is none, the program is synchronous, L15-L16) but because `revert_unstake` at L717 takes a fresh `borrow_mut`.

```rust
// L710-L720
match pay_out(owner, preview.net) {
    Ok(()) => { emit_vft_transfer(self.state, owner, ActorId::zero(), shares); self.emit_event(PoolEvent::Unstaked {...}).expect("event"); Ok(preview) }
    Err(e) => { self.state.borrow_mut().revert_unstake(owner, shares, &preview); Err(e) }
}
```
- **What:** Pay, then emit; or revert.
- **Assumes about `pay_out` (L672-L674)** -> `send_bytes_with_gas(to, [], 0, value)` (gstd basic.rs L588-L595) -> `gcore::msg::send_with_gas_delayed` (msg.rs L1023-L1050) -> `gr_send_wgas` syscall:
  - `Ok` means the message was *queued* with the value debited from the program; it does not mean `owner` received it. If `owner` is a program, a 0-gas message is dispatched to it and its handling is outside this file; what the runtime does with the value of an undeliverable or failed 0-gas message is not established here.
  - `Err` cases surfaced by the runtime include `InsufficientValue` (`0 < value < ED`, gear-core-errors L129-L132), `NotEnoughValue` (program balance below `value`, L50-L52), message-count/size limits. All are compensated by L717.
  - `as_value` (L666-L668) clamps `> u128::MAX` to `u128::MAX` silently.
- **Establishes:** on `Ok`, the counters already reflect the exit and the VARA is in flight; on `Err`, the counters are restored and no events are emitted.

**Cross-Function Dependencies:**
- Callee `PoolState::actor_for`, `rate_at`, `unstake`, `revert_unstake` (internal): see records. The handler relies on `unstake` having no writes on its `Err` paths, and on `revert_unstake` being an exact inverse.
- Callee `pay_out` (internal) -> `send_bytes_with_gas` (external-source-available, gstd/gcore 1.10.0): synchronous queueing; errors are `gcore::errors::Error` formatted into `TransferFailed(String)` (L673).
- Callee `emit_vft_transfer`, `emit_event` (internal/external): trap on failure after the payout is queued.
- Callers: dispatcher only.
- Shared state: as `PoolState::unstake`.
- Invariant couplings: `reserve` is decremented at queue time (L381); a later failure to deliver value returns it to the program balance without touching `reserve`, so program balance and `reserve` can drift apart upward. `unstake` cannot be used to drain more than `distributable` (L378), regardless of the destination.

**Open Questions:**
- unclear; need to inspect the Gear runtime for (a) what happens to a 0-gas value-bearing message whose destination is a program, and (b) whether a trap at L712-L713 cancels the message queued at L710 and reverts the state written at L707.
- unclear; the chain's existential deposit versus the smallest `net` this handler can attempt (`MIN_STAKE` at L30 is 0.01 VARA; a full exit of such a position pays below that after the fee).
