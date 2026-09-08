## `VaultState::shares_for` in programs/vault/app/src/lib.rs (L266-L271)

**Purpose:** Shares to mint for `assets` at `now_ms`, rounded down. It is the deposit price. Called pre-await by `check_deposit` (L382, as a validation that the deposit is not dust) and post-await by `settle_deposit` (L390, as the price actually paid); the two calls can see different state.

**Inputs & Assumptions:**
- `assets` (U256): the deposit. Trust: untrusted — comes from the `deposit`/`fund_rewards` message payload; `check_deposit` L380-L381 bounds it below by `MIN_DEPOSIT`, nothing bounds it above except `checked_mul`.
- `now_ms` (u64): trusted.
- Implicit state read: `total_shares`, `base_rate`, `distributable`.
- Precondition: `base_rate != 0` on the empty branch (L268 divides by it). Established by the constructor L219 (`>= SCALE`); by `burn_shares` L362 only to the extent that the pre-burn `rate_at` is non-zero — nothing found that guarantees that (see `vault-rate_at.md`).
- Precondition: `distributable != 0` on the non-empty branch (L270 divides by it). Established by `empty` L256 returning false.

**Outputs & Effects:**
- Returns `Ok(shares)` rounded down, or `Err(Overflow)` when the intermediate product overflows (L268, L270). No writes.

**Block-by-Block:**

```rust
// L267-L268
if self.empty(now_ms) {
    return assets.checked_mul(SCALE).ok_or(VaultError::Overflow).map(|v| v / self.base_rate);
}
```
- **What:** With no shares (or nothing distributable), price at `base_rate`: `assets * 1e18 / base_rate`.
- **Why here:** Avoids the zero divisor of the live ratio and gives a continuing vault its configured starting price (constructor comment L1015-L1017, test L1268-L1273).
- **Assumes:** `base_rate` is the right price for the *first* depositor after an empty spell even though `distributable` may be non-zero at that moment (orphaned dust or rewards vested with nobody holding). `settle_deposit` handles that by calling `sweep_orphans` first (L389), which moves the orphaned `distributable` into `fees_accrued` so it cannot be captured; `check_deposit` does not sweep, so its pre-await quote can differ from the settled one only by the dust that sweeping removes from `distributable` — which, on this branch, is not used anyway.
- **Establishes:** `shares > 0` iff `assets * 1e18 >= base_rate`.
- **Depended on by:** `check_deposit` L383 rejects `shares == 0`; `settle_deposit` L390 floors at one share.

```rust
// L270
assets.checked_mul(self.total_shares).ok_or(VaultError::Overflow).map(|v| v / self.distributable(now_ms))
```
- **What:** Live price: `assets * total_shares / distributable`.
- **Why here:** Standard ERC-4626 shape; rounding down favours existing holders.
- **Assumes:** `distributable > 0` (established by `empty`). `assets * total_shares` fitting in 256 bits is enforced by `checked_mul`.
- **Establishes:** the depositor gets `<=` their pro-rata share.
- **Depended on by:** same callers.

**Cross-Function Dependencies:**
- Callee `empty`/`distributable` (internal): the caller relies on `empty == false` implying both `total_shares > 0` and `distributable > 0`, which L256 gives.
- Callers: `check_deposit` L382 (pre-await), `settle_deposit` L390 (post-await), `preview_deposit` L984 (query, `unwrap_or(0)`).
- Shared state: none written.
- Invariant couplings: between `check_deposit` and `settle_deposit` there is an await in `Vault::deposit` (L747). Any message that changes `total_shares`, `holdings`, `fees_accrued`, `unbonding_total`, or the vesting fields in that window changes the quote. `settle_deposit` does not re-apply `check_deposit`'s `shares.is_zero()` rejection; it mints `max(shares, 1)` (L390).

**Open Questions:**
- unclear; need to inspect whether a deposit whose live quote rounds to zero at settle time (rate rose during the await, or `assets` just at `MIN_DEPOSIT` against a high `base_rate`) is meant to receive one share — L390 does that, and a share's value is then `assets_for(1)`, which may exceed what was deposited by dust.
