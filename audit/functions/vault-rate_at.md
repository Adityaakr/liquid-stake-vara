## `VaultState::rate_at` in programs/vault/app/src/lib.rs (L260-L263), with `empty` (L255-L257)

**Purpose:** Assets per share scaled by `1e18`. It is what `info().rate` and `rate()` report, what `burn_shares` snapshots into `base_rate` when the last share leaves (L359, L362), and what the `Deposited`/`Redeemed` events carry (L752, L767). `shares_for` and `assets_for` do not call it; they apply the same `empty` test and the same ratio themselves (L267-L270, L275-L278).

**Inputs & Assumptions:**
- `now_ms` (u64): block timestamp. Trust: trusted.
- Implicit state read: `total_shares`, `base_rate`, and everything `distributable` reads.
- Precondition for L262: `total_shares != 0`. Established by `empty` at L256 returning false only when `total_shares` is non-zero.

**Outputs & Effects:**
- Returns `U256`. No writes.
- Postcondition: when `empty`, returns `base_rate` (L261); otherwise `distributable * 1e18 / total_shares`, rounded down.

**Block-by-Block:**

```rust
// L255-L257
fn empty(&self, now_ms: u64) -> bool {
    self.total_shares.is_zero() || self.distributable(now_ms).is_zero()
}
```
- **What:** The vault "prices at `base_rate`" in two situations: no shares, or shares but nothing distributable.
- **Why here:** Shared by `rate_at`, `shares_for`, `assets_for` so the three agree on when the fallback price applies.
- **Assumes:** that pricing outstanding shares at `base_rate` while `distributable == 0` is intended. With `total_shares > 0` and `distributable == 0`, `assets_for` at L275-L276 still quotes `shares * base_rate / SCALE` for shares that have no backing, and `shares_for` at L267-L268 mints new shares at `base_rate` while existing shares are worthless. Nothing found that prevents the combination (`holdings` can be driven to exactly `fees + unbonding + locked` by a full redeem/unbond of all but rounding dust, see `vault-begin_redeem.md`); the payout side is still bounded by the reserve checks at L405 and L449, which compare against `distributable`, not against the quoted price.
- **Establishes:** the branch taken at L261, L267, L275.
- **Depended on by:** L261-L262.

```rust
// L261-L262
if self.empty(now_ms) { return self.base_rate; }
self.distributable(now_ms).saturating_mul(SCALE) / self.total_shares
```
- **What:** The fallback or the live ratio.
- **Why here:** Division only after `empty` excluded a zero divisor.
- **Assumes:** `distributable * 1e18` does not saturate; `saturating_mul` caps it at `U256::MAX`, which would understate the rate only for `distributable > 2^256 / 1e18`.
- **Establishes:** the value events and `base_rate` capture.
- **Depended on by:** `burn_shares` L359 (snapshot before the burn), `deposit` L752, `redeem` L767, `info` L562, `rate` L979.

**Cross-Function Dependencies:**
- Callee `distributable` (internal): see its record; the caller relies on it never panicking (saturating arithmetic) and on `<= holdings`.
- Callers: `burn_shares` L359 — note it calls `rate_at` *before* debiting, so the snapshot is the pre-burn rate. In `begin_redeem` the burn (L406) happens before `holdings -= net` (L408) and `fees += fee` (L407), so `rate_before` is the rate at which the redeemer was paid.
- Shared state: `base_rate` is written by the constructor L219 (clamped to `>= SCALE`) and by `burn_shares` L362 (unclamped: whatever `rate_at` returned).
- Invariant couplings: `base_rate > 0` is needed by `shares_for` L268 (division by `base_rate`). The constructor guarantees `>= SCALE`. `burn_shares` writes `rate_before`, which is `distributable * SCALE / total_shares` when not empty; that is zero only if `total_shares > distributable * 1e18`. Nothing found that bounds `total_shares` relative to `distributable` other than the pricing itself; see `vault-shares_for.md` Open Questions.

**Open Questions:**
- unclear; need a bound on how far `rate_at` can fall below `SCALE`. `settle_deposit` L390 mints `max(shares, 1)`, so a deposit worth less than one share still mints one share, diluting by dust each time; no test pins the minimum rate.
