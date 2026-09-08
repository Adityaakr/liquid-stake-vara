## `PoolState::request_unbond` in programs/vara-pool/app/src/lib.rs (L415-L429)

**Purpose:** Timed exit: burn `shares` now, fix the VARA owed at the current rate, move it from the holders' pool into `unbonding_total`, and record an `Unbond` entry claimable after `unbond_period_ms`. No fee (contrast `unstake` L375).

**Inputs & Assumptions:**
- `owner` (ActorId): trusted, resolved by `actor_for` (L729).
- `shares` (U256): untrusted handler parameter (L725).
- `now_ms` (u64): clock.
- Implicit state read: `paused`, `balances`, `unbond_period_ms`, `next_unbond_id`, `unbonds`, pricing state.
- Preconditions:
  - `balance_of(owner) >= shares` (L418).
  - `unbond_period_ms` may be zero: the constructor (L200) and `set_config` (L466) accept any `u64`; nothing bounds it. With zero, `claimable_at == now_ms` (L425) and `take_unbond` accepts in the same block (L436, `now_ms < claimable_at` false), giving a fee-free instant exit.

**Outputs & Effects:**
- Writes (after all checks): `balances`, `total_shares`, possibly `base_rate` (via `burn_shares` L423); `unbonding_total += assets` (L424, checked); `next_unbond_id += 1` (L426, plain add); `unbonds[owner].push(entry)` (L427).
- Returns the `Unbond` (id, shares, assets, requested_at, claimable_at).
- Errors before writes: `Paused` L416, `ZeroAmount` L417/L420, `InsufficientShares` L418, `Overflow` L419, `InsufficientReserve` L422. After `burn_shares`: `Overflow` at L424 only if `unbonding_total + assets` overflows U256 (shares would then be burned with no entry recorded, persisting as a normal `Err` reply).

**Block-by-Block:**

```rust
// L416-L420
self.ensure_live()?;
if shares.is_zero() { return Err(PoolError::ZeroAmount); }
if self.balance_of(&owner) < shares { return Err(PoolError::InsufficientShares); }
let assets = self.assets_for(shares, now_ms)?;
if assets.is_zero() { return Err(PoolError::ZeroAmount); }
```
- **What:** Gates, then price at the current rate with no fee.
- **Establishes:** `assets > 0`, `shares <= total_shares`.

```rust
// L421-L422
let available = self.distributable(now_ms);
if assets > available { return Err(PoolError::InsufficientReserve { available }); }
```
- **What:** The owed amount must be distributable now.
- **Why here:** Moving `assets` into `unbonding_total` reduces `distributable` by the same amount (L230); the check keeps it non-negative. Same `empty()`-with-shares rejection as in `unstake`.

```rust
// L423-L428
self.burn_shares(owner, shares, now_ms)?;
self.unbonding_total = self.unbonding_total.checked_add(assets).ok_or(PoolError::Overflow)?;
let entry = Unbond { id: self.next_unbond_id, shares, assets, requested_at: now_ms, claimable_at: now_ms.saturating_add(self.unbond_period_ms) };
self.next_unbond_id += 1;
self.unbonds.entry(owner).or_default().push(entry.clone());
Ok(entry)
```
- **What:** Burn, reserve the VARA, record the claim.
- **Why here:** `burn_shares` first so its `rate_at` snapshot precedes the `unbonding_total` write. `reserve` is untouched: the VARA stays in the program until `take_unbond`.
- **Assumes:** `next_unbond_id += 1` does not overflow `u64` (unbounded but not reachable in practice; a wrap would let two entries share an id and `take_unbond`'s `position` at L435 would find the earlier one). Ids are unique across all owners (single counter).
- **Establishes:** an unbond position no longer earns (its shares are gone, its `assets` are fixed; test L1145-L1155); reserve-cover invariant preserved; the per-owner `Vec` grows by one with no cap (linear scan in `take_unbond` L435).

**Cross-Function Dependencies:**
- Callee `assets_for` (internal): `assets <= distributable` when `shares <= total_shares`.
- Callee `distributable`, `burn_shares` (internal): see records.
- Callers: `Pool::request_unbond` L730; emits `Transfer(owner -> 0)` and `UnbondRequested` after (L732-L733). No payout in the handler.
- Shared state: `unbonds` and `unbonding_total` also written by `take_unbond` L439-L441 and `restore_unbond` L448-L452.
- Invariant couplings: `unbond_period_ms == 0` collapses the timed exit into a fee-free instant exit (admin-controlled via `set_config` L466, no lower bound).

**Open Questions:**
- unclear; whether `unbond_period_secs == 0` is an intended configuration (L463-L469 impose no bound while fee and vesting are bounded).
