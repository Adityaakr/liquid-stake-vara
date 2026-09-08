## `PoolState::fund` in programs/vara-pool/app/src/lib.rs (L396-L413)

**Purpose:** Add `assets` of VARA to the vesting tranche and to `reserve`. Whatever of the previous tranche is still locked is folded into the new one, and the new tranche's length is the amount-weighted average of the old time left and a full vesting period (comment L392-L395).

**Inputs & Assumptions:**
- `assets` (U256): message value (L762-L764). Trust: untrusted amount from any sender; `fund_rewards` has no access control (comment L759, L761-L767).
- `now_ms` (u64): clock.
- Implicit state read: `vesting_period_ms`, vest fields via `locked_rewards`.
- Preconditions:
  - The VARA is in the program balance (same as `stake`; handler L762).
  - No pause check: `fund` does not call `ensure_live`; funding works while paused.

**Outputs & Effects:**
- Writes `vest_amount`, `vest_start_ms`, `vest_end_ms` (L408-L410), then `reserve += assets` (L411, checked).
- Errors: `ZeroAmount` L397, `Overflow` L400 or L411. On the L411 `Overflow` path the vest fields have already been rewritten (persisting partial write; reachable only on U256 overflow of the reserve).
- Postcondition at `now_ms`: `locked_rewards(now_ms) == vest_amount == remaining + assets` (L222, since `vest_start_ms == now_ms`), so `distributable(now_ms)` is unchanged by the call. Already-vested rewards stay vested.

**Block-by-Block:**

```rust
// L397-L400
if assets.is_zero() { return Err(PoolError::ZeroAmount); }
let remaining = self.locked_rewards(now_ms);
let period = U256::from(self.vesting_period_ms);
let total = remaining.checked_add(assets).ok_or(PoolError::Overflow)?;
```
- **What:** Reject zero; read what is still locked; the new tranche is old-locked plus new.
- **Assumes:** `locked_rewards` returns zero for a finished tranche (L221) so a fresh tranche gets a full period at L401-L402.

```rust
// L401-L407
let time_left = if remaining.is_zero() {
    period
} else {
    let left = U256::from(self.vest_end_ms.saturating_sub(now_ms));
    (remaining.saturating_mul(left).saturating_add(assets.saturating_mul(period))) / total
};
let time_left = time_left.max(U256::one()).low_u64();
```
- **What:** Weighted average of `left` (weight `remaining`) and `period` (weight `assets`); at least 1 ms.
- **Why here:** Computed before the vest fields are overwritten (L408-L410) because it reads `vest_end_ms`.
- **Assumes:** `total > 0` (true: `assets > 0`). `left >= 1` when `remaining > 0` (L221 guarantees `now_ms < vest_end_ms`). `low_u64` truncation is exact because `time_left <= max(left, period) < 2^64`.
- **Establishes:** `1 <= time_left <= max(left, period)`. A large `assets` relative to `remaining` moves `time_left` toward a full `period`, so the still-locked remainder of the old tranche now releases over a longer time than it had left; a small `assets` leaves `time_left` near `left` (test L1098-L1119). Any sender can trigger either effect at the cost of the VARA they attach.

```rust
// L408-L411
self.vest_amount = total;
self.vest_start_ms = now_ms;
self.vest_end_ms = now_ms.saturating_add(time_left);
self.reserve = self.reserve.checked_add(assets).ok_or(PoolError::Overflow)?;
```
- **What:** Install the tranche; book the VARA.
- **Establishes:** `vest_end_ms > vest_start_ms` (needed by `locked_rewards` L223 and `apy_bps` L268); reserve-cover invariant preserved (reserve and locked both grew by `assets`, locked also absorbed `remaining` which was already inside it).
- **Depended on by:** every later `locked_rewards`/`distributable` call.

**Cross-Function Dependencies:**
- Callee `locked_rewards` (internal, L220-L226): depended on to return 0 iff the tranche is finished or absent, and `<= vest_amount` otherwise.
- Callers: `Pool::fund_rewards` L767 (refunds value on `Err`, L774; emits `RewardsFunded` with the post-state `reserve` and `vest_end_ms`, L767, L771).
- Shared state: vest fields (only writer), `reserve`, `vesting_period_ms` (written by `set_config` L467 and the constructor L201; a config change affects only subsequent `fund` calls, L399).
- Invariant couplings: funding an empty pool (`total_shares == 0`) creates rewards nobody holds; the part that vests before the next stake is swept to fees by `sweep_orphans` (L349-L354), the rest vests to the next holders.

**Open Questions:**
- unclear; whether the clock-reversal case (a later block with a smaller timestamp than `vest_start_ms`) is possible on the target chain; `fund` sets `vest_start_ms = now_ms` unconditionally (L409), so a backwards clock at the next `locked_rewards` call re-locks the whole tranche (L222).
