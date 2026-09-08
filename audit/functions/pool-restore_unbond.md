## `PoolState::restore_unbond` in programs/vara-pool/app/src/lib.rs (L447-L453)

**Purpose:** Put an `Unbond` entry back after `take_unbond` released it and the payout failed (`claim` L751-L753).

**Inputs & Assumptions:**
- `owner`, `entry`: exactly what `take_unbond` returned in the same execution (L744, L752). Trust: trusted.
- Preconditions: state unchanged since `take_unbond` (synchronous; only `pay_out` ran in between, L746).

**Outputs & Effects:**
- `unbonding_total += entry.assets` (L448, saturating), `reserve += entry.assets` (L449, saturating), push the entry and sort the owner's list by id (L450-L452).
- Postcondition: counters and the entry are back; the list order is by id ascending (the original insertion order, since ids are monotonic L425-L426).

**Block-by-Block:**

```rust
// L448-L449
self.unbonding_total = self.unbonding_total.saturating_add(entry.assets);
self.reserve = self.reserve.saturating_add(entry.assets);
```
- **What:** Inverse of L441-L442.
- **Assumes:** no overflow; saturating would silently under-restore.

```rust
// L450-L452
let list = self.unbonds.entry(owner).or_default();
list.push(entry);
list.sort_by_key(|u| u.id);
```
- **What:** Re-insert; restore ordering.
- **Why here:** `take_unbond` may have removed the owner's whole map entry (L440); `entry().or_default()` recreates it.
- **Assumes:** ids unique (single counter, L425-L426), so the sort is stable in meaning.

**Cross-Function Dependencies:**
- Callers: `Pool::claim` L752 only.
- Shared state: `unbonds`, `unbonding_total`, `reserve`.
- Invariant couplings: same as `revert_unstake`: compensates only the synchronous `Err` from `send_bytes_with_gas`, not a later delivery failure.

**Open Questions:**
- none.
