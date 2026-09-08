## `PoolState::burn_shares` in programs/vara-pool/app/src/lib.rs (L336-L343)

**Purpose:** Remove `value` shares from `from` and from the supply; when the supply reaches zero, freeze the pre-burn rate into `base_rate` so the receipt keeps its price through an empty spell (comment L160-L161, L335).

**Inputs & Assumptions:**
- `from` (ActorId): whose balance. Trust: trusted (callers pass the resolved owner, L379, L423).
- `value` (U256): shares to burn. Trust: untrusted upstream, but both callers verified `balance_of(from) >= value` (L374, L418) before calling.
- `now_ms` (u64): clock, used only for `rate_at` at L338.
- Implicit state read: `balances`, `total_shares`, and everything `rate_at` reads.
- Preconditions:
  - `total_shares >= value`: needs `sum(balances) <= total_shares`. Established by `mint_shares` (L329-L331: supply and balance grow by the same `value`; supply computed first with `checked_add`, balance credited, supply stored) and by this function (L339-L340). `revert_unstake` calls `mint_shares` and discards its error (L387); if that `checked_add` failed the balance would not be credited either (L329 runs first), so the pairing holds. Nothing else writes `total_shares`.
  - `rate_at(now_ms)` must be computed before the caller adjusts `reserve`/`fees`/`unbonding_total`: both callers call `burn_shares` before their bucket writes (L379 before L380-L381; L423 before L424), so `rate_before` is the pre-exit rate.

**Outputs & Effects:**
- Writes `balances[from]` (via `debit` L300-L301: removes the key at zero), `total_shares` (L340), and `base_rate` when the supply hits zero (L341).
- Errors: `ZeroAmount` (L337), `InsufficientShares` (L339, remapped from `debit`'s `InsufficientBalance`), `Overflow` (L340, only when `total_shares < value`).
- On the `Overflow` path the balance has already been debited (L339) and the supply not reduced: a partial write that persists because an `Err` return is a normal reply, not a trap (sails exposure.rs L109-L137; macros.rs L285-L293). Reachable only when the supply/balances pairing above is already broken.

**Block-by-Block:**

```rust
// L337-L338
if value.is_zero() { return Err(PoolError::ZeroAmount); }
let rate_before = self.rate_at(now_ms);
```
- **What:** Reject zero; snapshot the rate before any write.
- **Why here:** `rate_at` reads `total_shares` and `distributable`; taken before L339-L340 it is the rate holders saw on entry to this exit.
- **Depended on by:** L341.

```rust
// L339-L340
self.debit(from, value).map_err(|_| PoolError::InsufficientShares)?;
self.total_shares = self.total_shares.checked_sub(value).ok_or(PoolError::Overflow)?;
```
- **What:** Debit the holder, then the supply.
- **Assumes:** pairing invariant above for the second line to succeed.
- **Establishes:** `sum(balances)` and `total_shares` both reduced by `value`.

```rust
// L341
if self.total_shares.is_zero() { self.base_rate = rate_before; }
```
- **What:** Remember the rate for the empty spell.
- **Assumes:** `rate_before > 0` so later `shares_for` (L247) does not divide by zero. `rate_at` returns `base_rate` (> 0 by L198) on the empty path and `distributable * 1e18 / total_shares` otherwise (L241); the latter is zero only if `total_shares > distributable * 1e18`. Nothing here checks it.
- **Depended on by:** `rate_at` L240, `shares_for` L247, `assets_for` L255 during the empty spell.

**Cross-Function Dependencies:**
- Callee `rate_at` (internal, L239-L242): depended on for a positive value; see its record.
- Callee `debit` (internal, L299-L303): `checked_sub` on the balance; removes the map entry at zero. Error remapped at L339.
- Callers: `unstake` L379, `request_unbond` L423. Both assume it cannot fail after their own balance check, and both write `reserve`/`unbonding_total` only after it returns `Ok`.
- Shared state: `balances` also written by `credit`/`debit` through `transfer_shares` (L309-L310) and `mint_shares` (L330); `total_shares` by `mint_shares` L331.
- Invariant couplings: the last-share `base_rate` freeze plus `sweep_orphans` (L349-L354) together define what an empty pool prices at and who gets what accrued while it was empty.

**Open Questions:**
- unclear; whether `rate_before == 0` is reachable (integer division at L241 with `total_shares > distributable * 1e18`).
