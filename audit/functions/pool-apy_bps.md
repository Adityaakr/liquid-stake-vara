## `PoolState::apy_bps` in programs/vara-pool/app/src/lib.rs (L265-L271)

**Purpose:** Annualised release rate of the current tranche over distributable VARA, in basis points. Display only (`info` L520); nothing in the mint/burn/payout paths reads it.

**Inputs & Assumptions:**
- `now_ms` (u64): clock.
- Implicit state read: `distributable(now_ms)`, `vest_amount`, `vest_end_ms`, `vest_start_ms`, `total_shares`.
- Preconditions:
  - `vest_end_ms - vest_start_ms >= 1` (L268): established by `fund` L407 (`time_left >= 1`) and L409-L410; the `now_ms >= vest_end_ms` guard at L267 removes the only saturation corner. Unlike `locked_rewards`, there is no `now_ms <= vest_start_ms` early return, so `span` alone (not `now`) keeps the divisor non-zero; `d != 0` is checked at L267.

**Outputs & Effects:**
- `u32`, saturated at `u32::MAX` (L270). No writes.

**Block-by-Block:**

```rust
// L266-L267
let d = self.distributable(now_ms);
if self.vest_amount.is_zero() || now_ms >= self.vest_end_ms || d.is_zero() || self.total_shares.is_zero() { return 0; }
```
- **What:** Zero when nothing vests, the tranche has ended, nothing is distributable, or nobody holds shares.
- **Why here:** Guards both factors of the divisor at L269 (`span` via the `vest_end_ms` test, `d` directly).

```rust
// L268-L270
let span = U256::from(self.vest_end_ms - self.vest_start_ms);
let bps = self.vest_amount.saturating_mul(U256::from(YEAR_SECS * 1000)).saturating_mul(U256::from(BPS)) / (span.saturating_mul(d));
if bps > U256::from(u32::MAX) { u32::MAX } else { bps.low_u32() }
```
- **What:** `vest_amount / span` per ms, annualised, over `d`, in bps.
- **Assumes:** Products fit U256; `saturating_mul` caps silently on both numerator and denominator, which distorts rather than errors. `YEAR_SECS * 1000` is a `u64` constant product (3.15e10), no overflow.
- **Establishes:** result reflects the whole `vest_amount` over the whole `span`, not the remaining amount over the remaining time; after a `fund` fold (L405-L410) the two are the same at the funding instant only.

**Cross-Function Dependencies:**
- Callee `distributable` (internal).
- Callers: `info` L520.
- Shared state: reads only.

**Open Questions:**
- none.
