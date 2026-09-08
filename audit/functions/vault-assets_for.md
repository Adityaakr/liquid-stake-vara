## `VaultState::assets_for` in programs/vault/app/src/lib.rs (L274-L279)

**Purpose:** Assets owed for `shares` at `now_ms`, rounded down. It is the exit price for both instant redeems (through `preview_redeem` L295) and timed unbonds (L446), and the value shown by `position` L996.

**Inputs & Assumptions:**
- `shares` (U256): from the `redeem`/`request_unbond` payload or a balance. Trust: untrusted as a number; the callers separately check the owner holds at least that many (L401, L445) before relying on the result.
- `now_ms` (u64): trusted.
- Implicit state read: `total_shares`, `base_rate`, `distributable`.
- Precondition: `total_shares != 0` on the non-empty branch (L278 divides by it). Established by `empty` L256.

**Outputs & Effects:**
- Returns `Ok(assets)` or `Err(Overflow)` (L276, L278). No writes.
- Postcondition on the non-empty branch: `assets <= distributable` when `shares <= total_shares` (product divided by `total_shares`, rounded down).
- Postcondition on the empty branch: `assets = shares * base_rate / 1e18`, with **no relation to `distributable`**; the callers' reserve checks (L405, L449) are what stop such a quote from being paid.

**Block-by-Block:**

```rust
// L275-L276
if self.empty(now_ms) {
    return shares.checked_mul(self.base_rate).ok_or(VaultError::Overflow).map(|v| v / SCALE);
}
```
- **What:** Fallback price when there are no shares or nothing distributable.
- **Why here:** Symmetric with `shares_for`; keeps `preview_redeem`/`position` well-defined on an empty vault.
- **Assumes:** callers do not pay this quote without a reserve check. `begin_redeem` L405 and `request_unbond` L449 both check `assets <= distributable`; `position` L996 is a query.
- **Establishes:** nothing about solvency.
- **Depended on by:** the callers above.

```rust
// L278
shares.checked_mul(self.distributable(now_ms)).ok_or(VaultError::Overflow).map(|v| v / self.total_shares)
```
- **What:** `shares * distributable / total_shares`.
- **Why here:** After `empty` ruled out the zero divisor.
- **Assumes:** `shares <= total_shares` for the `<= distributable` bound. Established by the balance checks at L401/L445 together with the invariant `sum(balances) == total_shares` (see `vault-burn_shares.md`).
- **Establishes:** the bound that makes the reserve check at L405/L449 pass whenever `shares <= total_shares`, except through the `empty` branch.
- **Depended on by:** `preview_redeem` L295-L297 (`assets - fee` unchecked subtraction: needs `fee <= assets`, see below), `request_unbond` L446.

**Cross-Function Dependencies:**
- Callee `empty`/`distributable` (internal): as above.
- Callers: `preview_redeem` L295 — computes `fee = assets * instant_fee_bps / BPS` (L296) and `net = assets - fee` (L297) with an unchecked `-`. `fee <= assets` holds iff `instant_fee_bps <= BPS (10_000)`. `set_config` L506 enforces `<= MAX_FEE_BPS (1_000)`; the constructor L208-L220 stores `instant_fee_bps` **without any bound** — nothing found at construction. `U256`'s `Sub` panics on underflow, so a vault constructed with `instant_fee_bps > 10_000` panics in `preview_redeem` for any non-zero `assets`.
- Callers: `request_unbond` L446, `position` L996, `preview_redeem` L989 (query).
- Shared state: none written.
- Invariant couplings: unbond entries freeze `assets` at request time (L446, L452); later rate changes do not affect them, and they neither earn nor lose (test L1192-L1194).

**Open Questions:**
- unclear; need to inspect the deployment scripts for the `instant_fee_bps` value passed to `Program::new` L1018, since the constructor does not clamp it.
