## `VaultState::settle_deposit` in programs/vault/app/src/lib.rs (L388-L394)

**Purpose:** The post-await half of a deposit: once `pull_underlying` has succeeded, sweep orphans, price the deposit at the *current* state, mint shares and count the assets in `holdings`. It is the only place shares are minted for value and the only place `holdings` grows other than `fund`.

**Inputs & Assumptions:**
- `owner` (ActorId): resolved before the await (L743). Trust: semi-trusted (session mapping).
- `assets` (U256): the amount the token confirmed moving. Trust: trusted *if* the `Ok(Ok(true))` arm of `pull_underlying` implies the token committed a transfer of exactly `assets` — for the demo token it does (demo `transfer_from` L205-L214 either moves `value` and replies `Ok(true)` or traps).
- `now_ms` (u64): read again after the await (L750), so it is the settlement block, not the request block.
- Precondition: the tokens are in the vault. Established by `Vault::deposit` L747 returning early on any `Err` from `pull_underlying`.
- Precondition (not checked here): not paused, `assets >= MIN_DEPOSIT`. Established only pre-await by `check_deposit`; nothing re-checks after the await.

**Outputs & Effects:**
- State writes, in order: `fees_accrued` via `sweep_orphans` L389 (only when `total_shares == 0`); `balances[owner]`, `total_shares` via `mint_shares` L391; `holdings += assets` L392.
- Returns `Ok(shares)` or `Err(Overflow)` from L390 (`shares_for`), L391 (`mint_shares` L350 or `credit` L315), L392.
- Partial writes on error: if L391 fails, the sweep at L389 has already landed. If L392 fails, sweep and mint have landed and `holdings` has not. On every `Err` the tokens are already in the vault and `Vault::deposit` returns the error at L751 without minting or counting — the assets are then held but invisible to `holdings` (see `vault-distributable.md`).

**Block-by-Block:**

```rust
// L389
self.sweep_orphans(now_ms);
```
- **What:** If the vault is empty, move `distributable` to fees.
- **Why here:** Before pricing, so the empty-branch price is applied to a vault with `distributable == 0` and the new shares are backed only by `assets`.
- **Assumes:** see `vault-sweep_orphans.md`.
- **Establishes:** `distributable == 0` when `total_shares == 0`.
- **Depended on by:** L390 (which branch of `shares_for`), L392.

```rust
// L390
let shares = self.shares_for(assets, now_ms)?.max(U256::one());
```
- **What:** Re-price at settlement state and never mint zero.
- **Why here:** The state may have changed during the await (other deposits, redeems, funding, vesting, pause). Re-pricing here rather than using the pre-await quote is what keeps existing holders whole if the rate rose in between.
- **Assumes:** minting one share when the quote is zero is acceptable. That share is worth `assets_for(1)` afterwards, which is the live rate — possibly more than `assets` (dust-level gain for the depositor, dust-level dilution for others). `check_deposit` L383 rejected exactly this case pre-await; post-await it is floored instead.
- **Establishes:** `shares >= 1`.
- **Depended on by:** L391, the `Deposited` event L755, the return L756.

```rust
// L391-L392
self.mint_shares(owner, shares)?;
self.holdings = self.holdings.checked_add(assets).ok_or(VaultError::Overflow)?;
```
- **What:** Mint, then count the assets.
- **Why here:** `holdings` is updated last, after the mint priced against the *old* `holdings`; a deposit therefore prices itself out of the pre-deposit distributable (correct ERC-4626 order).
- **Assumes:** no path returns between L391 and L392 except the `Overflow` at L391, which leaves `holdings` unchanged while `balances` and `total_shares` are unchanged too (`mint_shares` writes `balances` before `total_shares`: `credit` L351 then L352; if `credit` overflows, `total_shares` was computed at L350 but not yet stored — no partial write inside `mint_shares`).
- **Establishes:** `holdings` includes the deposit; `distributable` rises by `assets`.
- **Depended on by:** every later price.

**Cross-Function Dependencies:**
- Callee `sweep_orphans`, `shares_for`, `mint_shares` (internal): records above.
- Callers: `Vault::deposit` L751 only, always after a successful `pull_underlying`.
- Shared state: `holdings`, `total_shares`, `balances`, `fees_accrued`.
- Invariant couplings: `holdings >= fees + unbonding + locked` preserved (adds only to `holdings`). `sum(balances) == total_shares` preserved by `mint_shares`. What happens between `check_deposit` and this function is recorded in `vault-Vault-deposit.md`.

**Open Questions:**
- unclear; need to inspect whether the intended behaviour for a paused vault is "in-flight deposits still settle" — `settle_deposit` has no `ensure_live`, so a `pause` landing during the await does not stop the mint.
