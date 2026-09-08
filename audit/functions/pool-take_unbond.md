## `PoolState::take_unbond` in programs/vara-pool/app/src/lib.rs (L432-L444)

**Purpose:** Remove a matured `Unbond` entry for `owner` and release its VARA from `reserve` and `unbonding_total`; the handler (`claim`, L739-L756) then pays it.

**Inputs & Assumptions:**
- `owner` (ActorId): trusted, resolved by `actor_for` with `SessionAction::Claim` (L743).
- `id` (u64): untrusted handler parameter (L739).
- `now_ms` (u64): clock.
- Implicit state read: `paused`, `unbonds`, `reserve`.
- Preconditions:
  - Entry belongs to `owner`: enforced by looking up `unbonds[owner]` first (L434) and searching only that list (L435); an id from another owner's list yields `UnbondNotFound` (test L1133).
  - Matured: `now_ms >= claimable_at` (L436).
  - `entry.assets <= reserve` (L437). Not compared with `unbonding_total` or `distributable`; relies on the reserve-cover invariant to guarantee that the VARA released here is the VARA reserved at request time and not holders' or locked VARA.
  - Not paused (L433): claims of already-matured unbonds are blocked while paused.

**Outputs & Effects:**
- Writes: removes the entry (L439), drops the owner's list when empty (L440), `unbonding_total -= assets` saturating (L441), `reserve -= assets` panicking `Sub` bounded by L437 (L442).
- Returns the removed entry (the handler pays `entry.assets` and passes the entry back to `restore_unbond` on failure).
- Errors, all before writes: `Paused` L433, `UnbondNotFound` L434/L435/L438, `UnbondNotReady` L436, `InsufficientReserve` L437.

**Block-by-Block:**

```rust
// L433-L437
self.ensure_live()?;
let list = self.unbonds.get(&owner).ok_or(PoolError::UnbondNotFound)?;
let idx = list.iter().position(|u| u.id == id).ok_or(PoolError::UnbondNotFound)?;
if now_ms < list[idx].claimable_at { return Err(PoolError::UnbondNotReady { claimable_at: list[idx].claimable_at }); }
if list[idx].assets > self.reserve { return Err(PoolError::InsufficientReserve { available: self.reserve }); }
```
- **What:** Locate by owner then id (linear), check maturity and reserve.
- **Why here:** All reads and checks under an immutable borrow before the mutable borrow at L438.
- **Establishes:** `assets <= reserve` for L442.

```rust
// L438-L443
let list = self.unbonds.get_mut(&owner).ok_or(PoolError::UnbondNotFound)?;
let entry = list.remove(idx);
if list.is_empty() { self.unbonds.remove(&owner); }
self.unbonding_total = self.unbonding_total.saturating_sub(entry.assets);
self.reserve -= entry.assets;
Ok(entry)
```
- **What:** Remove, then release the counters.
- **Assumes:** `unbonding_total >= entry.assets` (saturating otherwise). Holds because every entry's `assets` was `checked_add`ed at L424 and only removed here (L441) or re-added by `restore_unbond` (L448).
- **Establishes:** reserve-cover invariant preserved (both sides shrink by `assets`).

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal).
- Callers: `Pool::claim` L744; pays out after (L746) and calls `restore_unbond` on failure (L752).
- Shared state: `unbonds`, `unbonding_total`, `reserve`.
- Invariant couplings: the L437 check against `reserve` rather than `unbonding_total` means that if the reserve-cover invariant is broken (locked rewards re-growing on a backwards clock), a claim can still succeed and consume VARA that `distributable` or `locked_rewards` also counts.

**Open Questions:**
- unclear; whether pausing is meant to freeze matured claims (L433) — `collect_fees` and `fund` are not paused-gated (L831-L843, L396), the user exits and claims are.
