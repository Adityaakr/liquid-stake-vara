## `Faucet::claim` in programs/demo-token/app/src/lib.rs (L434-L447)

**Purpose:** Exported `Claim() -> Result<U256, TokenError>` (IDL L122). Unauthenticated, cooldown-limited mint to the caller. The only command in the program that returns its `Result` as a plain reply instead of a structured panic: it is `#[export]` without `unwrap_result` (L435), and the IDL shows `Result<...>` rather than `throws`.

**Inputs & Assumptions:**
- No payload.
- Implicit: `to = Syscall::message_source()` (L437) — any account or program; `now = Syscall::block_timestamp()` (L438); `paused`, `faucet_amount`, `faucet_cooldown_ms`, `last_claim`, `balances`, `total_supply`.
- Preconditions: none.

**Outputs & Effects:**
- Success: `mint(to, faucet_amount)` and `last_claim[to] = now` (L441 -> L145-L147); VFT `Transfer { 0, to, value }` under route 1 (L444); `Claimed { to, value, next_claim_at }` (L445); reply `Ok(value)` (L446).
- Error reply `Err(TokenError)`: `Paused`, `FaucetDisabled`, `FaucetCooldown { next_claim_at }`, or `Overflow` from `mint` (L139-L146). The `?` at L441 returns the `Err` as the handler's normal return value — no panic, so the reply is a successful message carrying an `Err` payload. No state was written on any of those paths (token-state-faucet-claim.md).

**Block-by-Block:**

```rust
// L439-L443
let (value, next_claim_at) = {
    let mut s = self.state.borrow_mut();
    let value = s.faucet_claim(to, now)?;
    (value, s.next_claim_at(&to))
};
```
- **What:** claim, then read back the new `next_claim_at` under the same borrow.
- **Why here:** `next_claim_at` is computed after `last_claim` was written (L147), so the event reports the post-claim value `now + cooldown` (L152).
- **Assumes:** `now` is in the same unit as `faucet_cooldown_ms` (milliseconds) — see open question in token-state-faucet-claim.md.
- **Establishes:** `value == faucet_amount`; `next_claim_at == now + cooldown` (saturating).
- **Depended on by:** the `Claimed` event (L445) and clients polling `Faucet::next_claim_at` (L456-L459).

```rust
// L444-L446
emit_vft_transfer(self.state, ActorId::zero(), to, value);
self.emit_event(FaucetEvent::Claimed { to, value, next_claim_at }).expect("event");
Ok(value)
```
- **What:** two events, then the amount.
- **Why here:** after the borrow block. Emit failure panics after the writes (atomicity assumption, token-vft-transfer-from.md open question).
- **Assumes:** `VFT_ROUTE_IDX` correct (token-emit-vft-transfer.md).
- **Establishes:** a claim is visible as a VFT `Transfer` from zero (tests/gtest.rs L63-L64).

**Cross-Function Dependencies:**
- Callee `faucet_claim` (internal, L138-L149): establishes pause, enabled, cooldown, and the mint; no partial writes on error.
- Callee `next_claim_at` (internal, L151-L153).
- Callee `emit_vft_transfer` (internal, L247-L252); `emit_event` (external-source-available, events.rs L92-L101).
- Callers: anyone, including programs. The vault does not call it (vault lib.rs L716, L726 are the only token calls).
- Shared state: `last_claim` (written only here via L147), `faucet_*` (set by `set_faucet`, `Program::new`), `balances`, `total_supply`.
- Invariant couplings: the error path does not consume gas differently from success in any way the code controls; a caller inside the cooldown gets `Err(FaucetCooldown)` as a normal reply. Because `claim` mints to arbitrary `message_source()`s with a per-account cooldown only, total supply grows with the number of distinct claimants (each new ActorId skips the cooldown branch, L141).

**Open Questions:**
- None beyond the timestamp unit and atomicity questions recorded elsewhere.
