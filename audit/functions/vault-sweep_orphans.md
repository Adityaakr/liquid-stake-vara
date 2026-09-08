## `VaultState::sweep_orphans` in programs/vault/app/src/lib.rs (L370-L375)

**Purpose:** When the vault has no shares, whatever is `distributable` belongs to nobody (rounding dust left by the last exit, rewards that vested while empty). This moves it into `fees_accrued` so the next depositor cannot capture it (comment L368-L369, test L1145-L1146). It is called only from `settle_deposit` L389, i.e. after a successful inbound transfer.

**Inputs & Assumptions:**
- `now_ms` (u64): trusted.
- Implicit state read: `total_shares`, `distributable(now_ms)`.
- Precondition: none.

**Outputs & Effects:**
- State write: `fees_accrued += distributable(now_ms)` (L373, `saturating_add`), only when `total_shares == 0` (L371).
- Postcondition: on the write path `distributable(now_ms) == 0` immediately after (holdings unchanged, fees raised by exactly the old distributable, and `distributable` uses `saturating_sub` so it floors at zero).

**Block-by-Block:**

```rust
// L371-L374
if self.total_shares.is_zero() {
    let orphaned = self.distributable(now_ms);
    self.fees_accrued = self.fees_accrued.saturating_add(orphaned);
}
```
- **What:** Move orphaned assets to the fee bucket.
- **Why here:** Called before `shares_for` in `settle_deposit` (L389-L390) so that the empty-branch price (`base_rate`) is applied against a vault whose `distributable` is zero; the depositor's shares are then backed by exactly the depositor's assets (L392) and the still-locked part of any running tranche vests to them afterwards (test L1147-L1148).
- **Assumes:** that rewards vested during an empty spell belong to the admin rather than to the next holder — a policy decision encoded here.
- **Establishes:** `distributable == 0` at the moment the first shares are minted.
- **Depended on by:** `settle_deposit` L390-L392.

**Cross-Function Dependencies:**
- Callee `distributable` (internal).
- Callers: `settle_deposit` L389 only. Not called by `fund` (funding an empty vault leaves the vested part orphaned until a deposit), by `collect_fees` (an admin cannot collect orphans before someone deposits), or by the exit paths that empty the vault (`begin_redeem`, `request_unbond` leave dust in `distributable` for the next deposit to sweep).
- Shared state: `fees_accrued` with `begin_redeem`, `revert_redeem`, `take_fees`, `restore_fees`.
- Invariant couplings: runs post-await in `Vault::deposit`; the sweep amount is whatever `distributable` is at settle time, which can include rewards vested during the await window.

**Open Questions:**
- none.
