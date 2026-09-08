## `PoolState::assets_for` in programs/vara-pool/app/src/lib.rs (L253-L258)

**Purpose:** VARA owed for `shares` at `now_ms`, rounded down. It is the exit price for both exit paths (`preview_unstake` L274 via `unstake` L375; `request_unbond` L419) and the `position` query (L935).

**Inputs & Assumptions:**
- `shares` (U256): amount being redeemed. Trust: untrusted (handler parameter L701/L725, or query L932-L935).
- `now_ms` (u64): clock.
- Implicit state read: `total_shares`, `base_rate`, `distributable(now_ms)`.
- Preconditions:
  - `total_shares != 0` on the L257 path: established by `empty` L235.
  - The function does not require `shares <= balance_of(owner)` or `shares <= total_shares`; both callers check the balance before calling (L374, L418). `preview_unstake`/`position` queries do not.

**Outputs & Effects:**
- `Ok(assets)` or `Err(Overflow)` (L255, L257). No writes.

**Block-by-Block:**

```rust
// L254-L255
if self.empty(now_ms) {
    return shares.checked_mul(self.base_rate).ok_or(PoolError::Overflow).map(|v| v / SCALE);
}
```
- **What:** Empty pool: `shares * base_rate / 1e18`.
- **Why here:** Guards the L257 division.
- **Assumes:** On the "shares > 0 but distributable == 0" sub-case this returns a positive amount for shares backed by nothing distributable. Both mutating callers then compare against `distributable`, which is zero, and refuse (L377-L378, L421-L422). The `position` query (L935) reports the positive amount.
- **Establishes:** nothing about solvency; the ceiling checks are the callers'.

```rust
// L257
shares.checked_mul(self.distributable(now_ms)).ok_or(PoolError::Overflow).map(|v| v / self.total_shares)
```
- **What:** Pro-rata: `shares * distributable / total_shares`, rounded down.
- **Establishes:** `assets <= distributable` whenever `shares <= total_shares`. When `shares > total_shares` (a query, or a caller that skipped the balance check) the result can exceed `distributable`.
- **Depended on by:** `unstake` L375-L378 and `request_unbond` L419-L422 (both re-check against `distributable` anyway).

**Cross-Function Dependencies:**
- Callee `empty`, `distributable` (internal): non-zero divisor on the L257 path.
- Callers: `preview_unstake` L274, `request_unbond` L419, `Pool::position` L935, tests.
- Invariant couplings: rounding down here plus rounding down in `shares_for` is what makes the same-block round trip non-profitable (test L1013-L1031). The instant fee is applied on top in `preview_unstake` L275-L276, not here.

**Open Questions:**
- none beyond the `base_rate` question shared with `rate_at`.
