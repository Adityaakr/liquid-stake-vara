## `Admin::set_faucet` in programs/demo-token/app/src/lib.rs (L338-L350)

**Purpose:** Exported `SetFaucet(amount, cooldown_secs) -> bool throws TokenError` (IDL L91). Reconfigures the open faucet; `amount == 0` disables it (comment L338, enforced at L140).

**Inputs & Assumptions:**
- `amount` (U256): per-claim mint. Trust: untrusted argument from the admin. No upper bound.
- `cooldown_secs` (u64): converted to milliseconds with `saturating_mul(1000)` (L346). Trust: untrusted. Zero is accepted (no cooldown, see token-state-faucet-claim.md); values above `u64::MAX / 1000` saturate to `u64::MAX` ms.
- Implicit: `caller = message_source()` (L341); `admin`.
- Preconditions: caller is admin — `ensure_admin` L344. Not gated by pause.

**Outputs & Effects:**
- Success: `faucet_amount = amount`, `faucet_cooldown_ms = cooldown_secs * 1000` saturating (L345-L346); `FaucetConfigured { amount, cooldown_secs }` (L348) — reports the requested seconds, not the stored (possibly saturated) value; reply `true` (L349).
- Error: `Unauthorized` (L344), structured panic (L339).

**Block-by-Block:**

```rust
// L344-L346
s.ensure_admin(caller)?;
s.faucet_amount = amount;
s.faucet_cooldown_ms = cooldown_secs.saturating_mul(1000);
```
- **What:** authorize, two writes.
- **Why here:** both writes after the check, inside one borrow.
- **Assumes:** the same conversion as `Program::new` L480 — established by inspection (identical expression).
- **Establishes:** the faucet parameters read at L140, L142, L145. Existing `last_claim` entries are not touched, so a shortened cooldown immediately lets past claimants claim again and a lengthened one pushes their `next_claim_at` (L152) out.
- **Depended on by:** `faucet_claim`, `next_claim_at`, `Faucet::config` (L452, which divides back by 1000 — after saturation the round trip does not return the input).

**Cross-Function Dependencies:**
- Callee `ensure_admin` (internal, L155-L157); `emit_event`.
- Callers: admin only (tests/gtest.rs L137-L142).
- Shared state: `faucet_amount`, `faucet_cooldown_ms` (also `Program::new` L479-L480).
- Invariant couplings: see token-state-faucet-claim.md for what the cooldown does and does not bound.

**Open Questions:**
- None.
