## `VaultState::restore_unbond` in programs/vault/app/src/lib.rs (L474-L480)

**Purpose:** Compensation for `take_unbond` after a failed payout: re-lock the assets, put them back in `holdings`, re-insert the entry in id order.

**Inputs & Assumptions:**
- `owner`, `entry`: captured before the await (L803-L809). Trust: internal.
- Precondition: the token did not pay `entry.assets` to `owner`. Same caveat as `revert_redeem`: established for `Ok(Ok(false))`, `Ok(Err(_))` and non-timeout transport errors; for `Timeout` nothing found.

**Outputs & Effects:**
- State writes: `unbonding_total += entry.assets` L475, `holdings += entry.assets` L476 (both saturating), `unbonds[owner]` push + sort L477-L479.
- Cannot fail.

**Block-by-Block:**

```rust
// L475-L479
self.unbonding_total = self.unbonding_total.saturating_add(entry.assets);
self.holdings = self.holdings.saturating_add(entry.assets);
let list = self.unbonds.entry(owner).or_default();
list.push(entry);
list.sort_by_key(|u| u.id);
```
- **What:** Reverse of L466-L469.
- **Why here:** The sort keeps the list ordered by id even though other requests may have been appended during the await; `take_unbond` does not rely on order (it searches by id, L462), so the sort is cosmetic for the `unbonds` query (L1000-L1002).
- **Assumes:** the entry was not re-created by anyone else (ids are monotone, L453) and the owner's `paused`/session state is irrelevant to restoring (no gates here).
- **Establishes:** state as before `take_unbond`, modulo whatever else changed in the window.
- **Depended on by:** a later `claim(id)`.

**Cross-Function Dependencies:**
- Callers: `Vault::claim` L816 only.
- Shared state: as `take_unbond`.
- Invariant couplings: accounting bound preserved (both terms up by the same amount).

**Open Questions:**
- same timeout question as `vault-revert_redeem.md`.
