## `VaultState::take_unbond` in programs/vault/app/src/lib.rs (L459-L471)

**Purpose:** Pre-await half of `claim`: find the owner's matured entry, remove it, release its lock and take its assets out of `holdings` before the outbound transfer (comment L458).

**Inputs & Assumptions:**
- `owner` (ActorId): from `actor_for` (L806). Trust: semi-trusted. The entry must be under this owner's key; another account's id is `UnbondNotFound` (L461-L462, gtest L245-L246).
- `id` (u64): payload. Trust: untrusted.
- `now_ms` (u64): trusted.
- Implicit state read: `paused`, `unbonds`, `holdings`.
- Precondition: none.

**Outputs & Effects:**
- State writes (only after all checks): `unbonds[owner]` entry removed, owner key removed when the list empties L466-L467; `unbonding_total -= entry.assets` saturating L468; `holdings -= entry.assets` unchecked L469.
- Returns the removed `Unbond`, or `Paused` L460, `UnbondNotFound` L461/L462/L465, `UnbondNotReady{claimable_at}` L463, `InsufficientReserve{available: holdings}` L464.

**Block-by-Block:**

```rust
// L460-L464
self.ensure_live()?;
let list = self.unbonds.get(&owner).ok_or(VaultError::UnbondNotFound)?;
let idx = list.iter().position(|u| u.id == id).ok_or(VaultError::UnbondNotFound)?;
if now_ms < list[idx].claimable_at { return Err(VaultError::UnbondNotReady { claimable_at: list[idx].claimable_at }); }
if list[idx].assets > self.holdings { return Err(VaultError::InsufficientReserve { available: self.holdings }); }
```
- **What:** Pause gate, lookup, maturity, reserve.
- **Why here:** All reads on an immutable borrow before the mutable re-borrow at L465.
- **Assumes:** `assets <= holdings` is the right reserve test for an unbond (not `distributable`): the entry's assets are already excluded from `distributable` via `unbonding_total`, so comparing to `holdings` asks "are the physical tokens there", which is what the transfer needs. Under the accounting bound, `holdings >= unbonding_total >= entry.assets` always, so this check fails only if the bound was broken.
- **Assumes:** a pause blocks claims of already matured unbonds (`ensure_live` at L460). That is a policy fact: paused holders cannot exit by either path.
- **Establishes:** `entry.assets <= holdings`, making L469 safe.
- **Depended on by:** L465-L470.

```rust
// L465-L470
let list = self.unbonds.get_mut(&owner).ok_or(VaultError::UnbondNotFound)?;
let entry = list.remove(idx);
if list.is_empty() { self.unbonds.remove(&owner); }
self.unbonding_total = self.unbonding_total.saturating_sub(entry.assets);
self.holdings -= entry.assets;
Ok(entry)
```
- **What:** Remove and release.
- **Why here:** Both `unbonding_total` and `holdings` drop by the same amount, so `distributable` is unchanged; an interleaved message during the await sees the same rate (test L1180 for the request side).
- **Assumes:** L465 cannot fail (the key existed at L461 and nothing ran in between).
- **Establishes:** the entry is gone before the transfer; a second `claim(id)` during the await gets `UnbondNotFound` (gtest L255-L256 for the after-case).
- **Depended on by:** `Vault::claim` L810-L819; `restore_unbond` on failure.

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal).
- Callers: `Vault::claim` L807 only.
- Shared state: `unbonds`, `unbonding_total`, `holdings`.
- Invariant couplings: accounting bound preserved. The compensating write is `restore_unbond`, which re-inserts the same entry (same `id`), so interleaved `request_unbond` calls (which only ever allocate new ids from `next_unbond_id`) cannot collide with it.

**Open Questions:**
- none.
