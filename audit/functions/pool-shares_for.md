## `PoolState::shares_for` in programs/vara-pool/app/src/lib.rs (L245-L250)

**Purpose:** Shares minted for `assets` at `now_ms`, rounded down. This is the deposit price; `stake` (L362) relies on it to keep existing holders' value per share from falling.

**Inputs & Assumptions:**
- `assets` (U256): amount being deposited. Trust: untrusted (from message value, L682-L683, or a query parameter L922).
- `now_ms` (u64): clock.
- Implicit state read: `total_shares`, `base_rate`, `distributable(now_ms)`.
- Preconditions:
  - `base_rate != 0` on the empty path (L247): constructor L198 gives `>= SCALE`; `burn_shares` L341 stores `rate_at`, which is `> 0` under the conditions discussed in `rate_at`. Nothing in this function checks it; a zero would trap on division.
  - `distributable(now_ms) != 0` on the non-empty path (L249): established by `empty` returning false (L235).

**Outputs & Effects:**
- Returns `Ok(shares)` or `Err(Overflow)` (checked multiplications L247, L249). May return `Ok(0)`; the caller `stake` rejects that at L363. No writes.

**Block-by-Block:**

```rust
// L246-L247
if self.empty(now_ms) {
    return assets.checked_mul(SCALE).ok_or(PoolError::Overflow).map(|v| v / self.base_rate);
}
```
- **What:** Empty pool prices at `base_rate`: `assets * 1e18 / base_rate`.
- **Why here:** Must precede L249, which would divide by a zero `distributable`.
- **Assumes:** `empty` covers both "no shares" and "shares but nothing distributable" (L235). In the second case the newly minted shares are added to an existing non-zero supply (L364, L329-L331) without any pro-rata relation to it.
- **Establishes:** `shares <= assets` because `base_rate >= SCALE` (L198) at construction; after a `burn_shares` overwrite (L341) this holds only if the stored rate is `>= SCALE`.

```rust
// L249
assets.checked_mul(self.total_shares).ok_or(PoolError::Overflow).map(|v| v / self.distributable(now_ms))
```
- **What:** Pro-rata: `assets * total_shares / distributable`, rounded down.
- **Assumes:** `distributable(now_ms)` is the same value `empty` just tested (same `now_ms`, no writes in between; true because both are `&self`).
- **Establishes:** value per share after the mint is `>=` value per share before (rounding favours existing holders).
- **Depended on by:** `stake` L362-L364; `Pool::preview_stake` L923 (which maps `Err` to zero).

**Cross-Function Dependencies:**
- Callee `empty` (internal, L234-L236) and `distributable` (internal, L229-L231): depended on for the non-zero divisor on the L249 path.
- Callers: `stake` L362 (rejects zero L363; calls `sweep_orphans` first L361 so that on the `total_shares == 0` path `distributable` is zero and the `base_rate` branch is taken); `Pool::preview_stake` L923.
- Shared state: reads only.
- Invariant couplings: with `sweep_orphans` having run, the L246 branch is the only one reachable when `total_shares == 0`; without the sweep, a first staker would take L249 with `total_shares == 0` and receive zero shares.

**Open Questions:**
- unclear; whether `base_rate` can be zero (see `rate_at`).
