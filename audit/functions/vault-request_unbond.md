## `VaultState::request_unbond` in programs/vault/app/src/lib.rs (L442-L456)

**Purpose:** The timed exit: burn `shares` now, freeze their value at today's rate as an unbond entry, and lock that amount out of `distributable` (`unbonding_total`) until `claimable_at`. Synchronous; no token movement here.

**Inputs & Assumptions:**
- `owner` (ActorId): from `actor_for` (L791). Trust: semi-trusted.
- `shares` (U256): payload. Trust: untrusted.
- `now_ms` (u64): trusted.
- Implicit state read: `paused`, `balances`, pricing state, `next_unbond_id`, `unbond_period_ms`.
- Precondition: none beyond what the function checks.

**Outputs & Effects:**
- State writes, after the last `Err` return (except the unreachable `Overflow` at L450/L451): `balances`, `total_shares`, maybe `base_rate` (L450); `unbonding_total += assets` L451; `next_unbond_id += 1` L453; `unbonds[owner].push(entry)` L454.
- Returns the `Unbond` entry, or `Paused` L443, `ZeroAmount` L444/L447, `InsufficientShares` L445, `Overflow` L446/L450/L451, `InsufficientReserve` L449.

**Block-by-Block:**

```rust
// L443-L449
self.ensure_live()?;
if shares.is_zero() { return Err(VaultError::ZeroAmount); }
if self.balance_of(&owner) < shares { return Err(VaultError::InsufficientShares); }
let assets = self.assets_for(shares, now_ms)?;
if assets.is_zero() { return Err(VaultError::ZeroAmount); }
let available = self.distributable(now_ms);
if assets > available { return Err(VaultError::InsufficientReserve { available }); }
```
- **What:** Same gate sequence as `begin_redeem` minus the fee: pause, zero, balance, price, non-zero payout, reserve.
- **Why here:** The reserve check makes `unbonding_total += assets` keep `holdings >= fees + unbonding + locked` (see `vault-distributable.md`); it is also the only guard on the `empty` branch of `assets_for`.
- **Establishes:** `0 < assets <= distributable(now_ms)`.
- **Depended on by:** L451, and `take_unbond` later (which checks `assets <= holdings` again at L464).

```rust
// L450-L455
self.burn_shares(owner, shares, now_ms)?;
self.unbonding_total = self.unbonding_total.checked_add(assets).ok_or(VaultError::Overflow)?;
let entry = Unbond { id: self.next_unbond_id, shares, assets, requested_at: now_ms, claimable_at: now_ms.saturating_add(self.unbond_period_ms) };
self.next_unbond_id += 1;
self.unbonds.entry(owner).or_default().push(entry.clone());
Ok(entry)
```
- **What:** Burn, lock, record.
- **Why here:** Burn first so the rate snapshot in `burn_shares` is pre-exit; lock before recording so the entry is always backed.
- **Assumes:** `next_unbond_id += 1` on `u64` does not overflow — a plain `+=`; in a release wasm build it wraps. Ids are unique while it does not wrap; `take_unbond` finds entries by `id` within one owner's list (L462), and `restore_unbond` sorts by `id` (L479). 2^64 requests is not reachable in practice; nothing found that checks it.
- **Assumes:** the entry's `assets` will still be in `holdings` at claim time. `unbonding_total` is subtracted from `distributable`, so no later redeem/unbond can spend it (their reserve checks use `distributable`); `take_fees` is bounded by `fees_accrued`, also excluded from `distributable`. Established by the accounting bound.
- **Establishes:** an entry with `claimable_at = now + unbond_period_ms`; `unbond_period_ms` is whatever `set_config`/constructor stored (unbounded above, `saturating_mul` L221/L508; zero allowed, making unbonds claimable immediately at a zero fee — the fee-free exit path when `unbond_period_secs == 0`).
- **Depended on by:** `take_unbond`, `UnbondRequested` event L795.

**Cross-Function Dependencies:**
- Callees `ensure_live`, `assets_for`, `distributable`, `burn_shares` (internal).
- Callers: `Vault::request_unbond` L792 only.
- Shared state: `unbonds`, `unbonding_total`, `next_unbond_id` with `take_unbond`/`restore_unbond`; share state with the rest.
- Invariant couplings: an unbond entry is priced once and never re-priced; rewards vesting afterwards go to remaining holders only (test L1192-L1194). `set_config` changing `unbond_period_secs` does not affect existing entries.

**Open Questions:**
- unclear; need to inspect whether an unbounded `unbonds[owner]` list length matters for gas: `take_unbond` does a linear `position` (L462) and `restore_unbond` a sort (L479) on it, and nothing caps entries per owner.
