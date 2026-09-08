## `PoolState::unstake` in programs/vara-pool/app/src/lib.rs (L371-L383)

**Purpose:** Instant exit: burn `shares`, charge the instant fee, and reduce `reserve` by the net payout. The handler (L701-L721) then sends the VARA; this function only moves internal counters.

**Inputs & Assumptions:**
- `owner` (ActorId): trusted, resolved by `actor_for` (L705).
- `shares` (U256): untrusted handler parameter (L701).
- `now_ms` (u64): clock.
- Implicit state read: `paused`, `balances`, `instant_fee_bps`, and everything `assets_for`/`distributable` read.
- Preconditions:
  - `instant_fee_bps <= BPS` so `assets - fee` at L276 cannot underflow (U256 `Sub` panics on underflow). Established by `set_config` L464 (`<= MAX_FEE_BPS = 1000`) for values set after deployment; for the constructor argument (L188, L199) nothing found: `PoolState::new` stores `instant_fee_bps` unchecked.
  - `balance_of(owner) >= shares` (L374) so `burn_shares` cannot fail at L339.

**Outputs & Effects:**
- Writes (only after all checks): `balances[owner]`, `total_shares`, possibly `base_rate` (via `burn_shares` L379); `fees_accrued += fee` (L380); `reserve -= net` (L381).
- Returns the `UnstakePreview {assets, fee, net}` used by the handler to pay and, on failure, to revert (L710, L717).
- Errors: `Paused` L372, `ZeroAmount` L373/L376, `InsufficientShares` L374, `Overflow` L375 (from `assets_for`), `InsufficientReserve` L378. All before any write.

**Block-by-Block:**

```rust
// L372-L374
self.ensure_live()?;
if shares.is_zero() { return Err(PoolError::ZeroAmount); }
if self.balance_of(&owner) < shares { return Err(PoolError::InsufficientShares); }
```
- **What:** Pause gate, non-zero, holder has the shares.
- **Establishes:** `shares <= balance <= total_shares`, so `assets_for` returns `<= distributable` on the pro-rata path.

```rust
// L375-L376
let p = self.preview_unstake(shares, now_ms)?;
if p.net.is_zero() { return Err(PoolError::ZeroAmount); }
```
- **What:** Price the exit and the fee.
- **Assumes:** `preview_unstake` (L273-L277): `fee = assets * bps / 10000`, `net = assets - fee`. Needs the fee bound above.
- **Establishes:** `net > 0`; `fee + net == assets`.

```rust
// L377-L378
let available = self.distributable(now_ms);
if p.assets > available { return Err(PoolError::InsufficientReserve { available }); }
```
- **What:** The whole gross amount (fee included) must be distributable.
- **Why here:** Last check before writes. Because `distributable <= reserve` (L230), this also bounds `net` by `reserve`, which L381 needs.
- **Establishes:** after the writes, `distributable' = distributable - assets >= 0` and reserve-cover holds.

```rust
// L379-L382
self.burn_shares(owner, shares, now_ms)?;
self.fees_accrued = self.fees_accrued.saturating_add(p.fee);
self.reserve -= p.net;
Ok(p)
```
- **What:** Burn, book fee, release net from reserve.
- **Why here:** `burn_shares` first so its `rate_at` snapshot (L338) sees the pre-exit buckets. The `-=` at L381 is a panicking subtraction; bounded by L378.
- **Assumes:** `burn_shares` cannot fail here (its own checks are subsumed by L373-L374; its `checked_sub` at L340 needs the supply/balances pairing).
- **Establishes:** the fee stays inside `reserve` but outside `distributable`; only `net` is scheduled to leave.

**Cross-Function Dependencies:**
- Callee `preview_unstake` (internal, L273-L277) -> `assets_for` (L253-L258): depended on for `assets <= distributable` given `shares <= total_shares`; on the `empty()` sub-case with shares outstanding it returns `shares * base_rate / 1e18`, which L378 then rejects because `available == 0`.
- Callee `distributable`, `burn_shares` (internal): see their records.
- Callers: `Pool::unstake` L707. It reads `rate_at` before calling (L706) for the event, pays `p.net` after (L710), and calls `revert_unstake` on payout failure (L717).
- Shared state: `reserve`, `fees_accrued`, `balances`, `total_shares`, `base_rate`.
- Invariant couplings: the fee moves value from holders to the fee bucket at exit; `sweep_orphans` and the empty-spell `base_rate` decide what happens to rounding dust left behind when the last holder exits.

**Open Questions:**
- unclear; whether any deployment passes `instant_fee_bps > 10000` to the constructor (L957-L958). In-file, only `set_config` bounds it.
