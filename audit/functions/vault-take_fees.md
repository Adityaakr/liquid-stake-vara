## `VaultState::take_fees` in programs/vault/app/src/lib.rs (L483-L490)

**Purpose:** Pre-await half of `collect_fees`: zero the fee bucket and take the same amount out of `holdings` before the transfer to the admin's chosen recipient.

**Inputs & Assumptions:**
- No parameters. Implicit state read: `fees_accrued`, `holdings`.
- Precondition (checked by the caller, not here): the caller is admin (`ensure_admin` at L902). Nothing in this function checks identity; it is `pub` and any future caller would need to add the check.

**Outputs & Effects:**
- State writes (after both checks): `fees_accrued = 0` L487, `holdings -= amount` L488.
- Returns `Ok(amount)`, `Err(ZeroAmount)` L485, `Err(InsufficientReserve{available: holdings})` L486.

**Block-by-Block:**

```rust
// L484-L486
let amount = self.fees_accrued;
if amount.is_zero() { return Err(VaultError::ZeroAmount); }
if amount > self.holdings { return Err(VaultError::InsufficientReserve { available: self.holdings }); }
```
- **What:** Nothing to collect, or the bucket claims more than is held.
- **Why here:** L486 makes the unchecked `-=` at L488 safe. Under the accounting bound `holdings >= fees_accrued`, so it fails only if the bound was broken.
- **Establishes:** `0 < amount <= holdings`.
- **Depended on by:** L487-L488.

```rust
// L487-L488
self.fees_accrued = U256::zero();
self.holdings -= amount;
```
- **What:** Take the whole bucket.
- **Why here:** Both terms drop together so `distributable` is unchanged during the await.
- **Establishes:** fees credited by a `redeem` that lands during the await accumulate from zero and are not lost (`restore_fees` adds back, L493).
- **Depended on by:** `Vault::collect_fees` L907-L916.

**Cross-Function Dependencies:**
- Callers: `Vault::collect_fees` L904 only.
- Shared state: `fees_accrued` with `begin_redeem` L407, `revert_redeem` L415, `sweep_orphans` L373, `restore_fees` L493.
- Invariant couplings: with `revert_redeem`'s `saturating_sub` (L415), a `collect_fees` that lands between `begin_redeem` and a failed `push_underlying` collects a fee for a redeem that is then undone; see `vault-revert_redeem.md`.

**Open Questions:**
- none.
