## `TokenState::faucet_claim` in programs/demo-token/app/src/lib.rs (L138-L149)

**Purpose:** Open, rate-limited mint of `faucet_amount` to `who`. Backs `Faucet::claim` (L441). It is the only unauthenticated supply-increasing path.

**Inputs & Assumptions:**
- `who` (ActorId): `message_source()` at the only call site (L437). Trust: trusted identity, unrestricted set of callers (any account or program).
- `now_ms` (u64): `Syscall::block_timestamp()` at the call site (L438). Trust: trusted (runtime). Unit is milliseconds — established by the tests' arithmetic (tests/gtest.rs L12, L74) and by `faucet_cooldown_ms` being seconds x 1000 (L346, L480); the gcore implementation itself was not read.
- Implicit: `paused` (L139), `faucet_amount` (L140, L145), `faucet_cooldown_ms` (L142), `last_claim` (L141, L147).

**Outputs & Effects:**
- `Err(Paused)` (L139), `Err(FaucetDisabled)` when `faucet_amount == 0` (L140), `Err(FaucetCooldown { next_claim_at })` when `now_ms < last + cooldown` (L143): no writes.
- Errors from `mint` (L146): `ZeroAddress` (only if `who` is zero — not reachable from a message source), `Overflow` on total supply; no writes (mint writes nothing on its error paths).
- Success: balance and supply grow by `faucet_amount` (via `mint`), `last_claim[who] = now_ms` (L147). Returns the amount (L148).

**Block-by-Block:**

```rust
// L141-L144
if let Some(last) = self.last_claim.get(&who) {
    let next = last.saturating_add(self.faucet_cooldown_ms);
    if now_ms < next { return Err(TokenError::FaucetCooldown { next_claim_at: next }); }
}
```
- **What:** cooldown check for repeat claimants; first-time claimants skip it.
- **Why here:** after the disabled check, before the mint.
- **Assumes:** `faucet_cooldown_ms` is a meaningful bound. With `faucet_cooldown_ms == 0`, `next == last` and `now_ms >= last` always holds (block timestamps are non-decreasing), so every message — including several in the same block — passes. With a saturated `faucet_cooldown_ms == u64::MAX` (from `saturating_mul` at L346/L480), `next` saturates to `u64::MAX` and no second claim ever passes. Neither bound is rejected anywhere: `set_faucet` L345-L346 and `Program::new` L479-L480 store any values.
- **Establishes:** `now_ms >= last_claim[who] + cooldown` for repeat claimants.
- **Depended on by:** the write at L147 and `next_claim_at` (L151-L153), which recomputes the same `next`.

```rust
// L145-L147
let amount = self.faucet_amount;
self.mint(who, amount)?;
self.last_claim.insert(who, now_ms);
```
- **What:** mint then record the claim time.
- **Why here:** `last_claim` is written only after the mint succeeded, so a failed mint does not consume the cooldown.
- **Assumes:** `mint` writes nothing on failure (true: L121-L124 return before L125).
- **Establishes:** `last_claim[who] == now_ms`; one map entry per distinct claimant, never removed (no other writer or remover of `last_claim` in the file).
- **Depended on by:** `Faucet::claim`'s `next_claim_at` computation (L442).

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal, L79-L81).
- Callee `mint` (internal, L120-L128): establishes supply bookkeeping; the pause check runs twice (L139 and L121).
- Callers: `Faucet::claim` L441.
- Shared state: `faucet_amount`, `faucet_cooldown_ms` written by `set_faucet` L345-L346 and `Program::new` L479-L480; `last_claim` written here only.
- Invariant couplings: total supply is unbounded by anything but `U256` overflow (L124) and the per-account cooldown; distinct accounts are not rate-limited against each other.

**Open Questions:**
- unclear; need to inspect `gcore::exec::block_timestamp` to confirm the millisecond unit rather than relying on the tests' constants.
