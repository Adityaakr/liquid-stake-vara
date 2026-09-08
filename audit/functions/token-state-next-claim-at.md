## `TokenState::next_claim_at` in programs/demo-token/app/src/lib.rs (L151-L153)

**Purpose:** Read-only mirror of the cooldown arithmetic in `faucet_claim` (L142), used for the `Claimed` event (L442) and the `Faucet::next_claim_at` query (L458).

**Inputs & Assumptions:**
- `who` (&ActorId). Trust: untrusted (query argument) or trusted (message source in `claim`).
- Implicit: `last_claim`, `faucet_cooldown_ms`.

**Outputs & Effects:**
- `last_claim[who].saturating_add(faucet_cooldown_ms)` or `0` if never claimed (L152). No writes.

**Block-by-Block:**

```rust
// L152
self.last_claim.get(who).map(|l| l.saturating_add(self.faucet_cooldown_ms)).unwrap_or(0)
```
- **What:** same formula as L142.
- **Why here:** single expression.
- **Assumes:** the formula stays in sync with L142 — established by inspection; the two are not shared code.
- **Establishes:** nothing; pure read.
- **Depended on by:** `Faucet::claim` L442 (reads it after the claim so it reports the new `next`), `Faucet::next_claim_at` L458. Note the value reflects the current `faucet_cooldown_ms`; a later `set_faucet` changes the answer for past claims.

**Cross-Function Dependencies:**
- No callees. Callers above. Shared state: `last_claim` (written at L147), `faucet_cooldown_ms` (L346, L480).

**Open Questions:**
- None.
