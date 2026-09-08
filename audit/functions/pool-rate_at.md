## `PoolState::rate_at` in programs/vara-pool/app/src/lib.rs (L239-L242)

**Purpose:** VARA per kVARA scaled by 1e18. Reported by `rate` (L917-L919) and `info` (L519), recorded in `Staked`/`Unstaked` events (L687, L706), and remembered as `base_rate` when the last share is burned (L338, L341). The mint/burn math itself does not call it; `shares_for`/`assets_for` recompute the ratio directly.

**Inputs & Assumptions:**
- `now_ms` (u64): clock.
- Implicit state read: `total_shares`, `base_rate`, and everything `distributable` reads.
- Preconditions:
  - `total_shares > 0` on the L241 path: established by `empty` (L235) returning false only when `total_shares != 0`, so the division at L241 is never by zero.
  - `base_rate > 0` on the L240 path: constructor sets `base_rate >= SCALE` (L198). `burn_shares` overwrites it with the pre-burn `rate_at` (L338, L341); that value is `>= SCALE` as long as the rate never fell below its starting point, which holds under the non-decreasing-rate argument (mints round shares down L249, burns round assets down L257, `unstake` removes exactly `assets` from distributable L380-L381) but that argument needs monotonic time (see `locked_rewards`). Nothing in this function checks it.

**Outputs & Effects:**
- Returns `U256`. No writes.

**Block-by-Block:**

```rust
// L240
if self.empty(now_ms) { return self.base_rate; }
```
- **What:** With no shares, or with shares but zero distributable, price at `base_rate`.
- **Why here:** Guards the division at L241.
- **Assumes:** `empty` (L234-L236) is `total_shares == 0 || distributable == 0`. The second disjunct means a pool that has shares outstanding but nothing distributable reports `base_rate` rather than zero.
- **Depended on by:** `burn_shares` L338 (records this as the empty-spell rate), `Pool::stake` L687 and `Pool::unstake` L706 (event field only).

```rust
// L241
self.distributable(now_ms).saturating_mul(SCALE) / self.total_shares
```
- **What:** Distributable over shares, scaled.
- **Assumes:** `distributable * 1e18` fits U256 (`saturating_mul` caps silently otherwise; needs `distributable < 2^196`).
- **Establishes:** result can be zero when `total_shares > distributable * 1e18` (integer division). If that zero is then stored as `base_rate` by `burn_shares` L341, `shares_for` L247 and `assets_for` L255 divide by / multiply by zero. Whether `total_shares` can exceed `distributable * 1e18` with `distributable > 0`: shares are minted at `assets * 1e18 / base_rate <= assets` on the empty path (L247, `base_rate >= SCALE`) and pro rata otherwise (L249); nothing found that lets shares outgrow assets by 1e18 short of the clock-reversal path.

**Cross-Function Dependencies:**
- Callee `empty` (internal, L234-L236): depended on for `total_shares != 0` on the non-empty path; it delivers that on every path.
- Callee `distributable` (internal, L229-L231): see its record.
- Callers: `burn_shares` L338, `Pool::stake` L687, `Pool::unstake` L706, `info` L519, `Pool::rate` L918.
- Shared state: `base_rate` written at L198 and L341 only.
- Invariant couplings: the rate is a view; the mint/burn functions use the same ratio but with different rounding, so `rate_at` can differ from `assets_for(1 share)` by rounding.

**Open Questions:**
- unclear; whether the "shares > 0 and distributable == 0" state is reachable on a monotonic clock. If it is, `rate_at` reports `base_rate` for shares that hold nothing, and `shares_for` mints at `base_rate` alongside them (L246-L247).
