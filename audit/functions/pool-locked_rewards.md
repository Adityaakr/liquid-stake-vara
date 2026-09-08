## `PoolState::locked_rewards` in programs/vara-pool/app/src/lib.rs (L220-L226)

**Purpose:** Rewards of the current vesting tranche not yet released at `now_ms`. Everything priced in VARA-per-share flows through `distributable` (L230), which subtracts this value from the reserve; without it funded rewards would be claimable at once and a same-block round trip would capture them.

**Inputs & Assumptions:**
- `now_ms` (u64): caller-supplied clock. Trust: trusted in-program; every on-chain caller passes `Syscall::block_timestamp()` via `now_ms()` (L661-L663).
- Implicit state read: `vest_amount`, `vest_start_ms`, `vest_end_ms` (L221-L225). Written only by `fund` (L408-L410) and the constructor (L205-L207).
- Preconditions:
  - `vest_end_ms > vest_start_ms` whenever `vest_amount != 0`. Established by `fund`: `vest_start_ms = now_ms` (L409), `vest_end_ms = now_ms.saturating_add(time_left)` with `time_left >= 1` (L407, L410). Only violated when `saturating_add` saturates at `u64::MAX` and `now_ms == u64::MAX`, in which case L221 (`now_ms >= vest_end_ms`) returns first.
  - Block timestamps do not decrease between messages: nothing found in this file. If `now_ms` is smaller in a later message than in the `fund` that set `vest_start_ms`, L222 returns the full `vest_amount` again and previously released rewards become locked again.

**Outputs & Effects:**
- Returns a `U256` in `[0, vest_amount]`. No state writes, no messages.

**Block-by-Block:**

```rust
// L221
if self.vest_amount.is_zero() || now_ms >= self.vest_end_ms { return U256::zero(); }
```
- **What:** Nothing vesting, or the tranche has finished: nothing is locked.
- **Why here:** Runs before the subtraction at L224, so `vest_end_ms - now_ms` cannot underflow.
- **Establishes:** on fall-through, `now_ms < vest_end_ms` and `vest_amount > 0`.

```rust
// L222
if now_ms <= self.vest_start_ms { return self.vest_amount; }
```
- **What:** Before (or at) the tranche start everything is locked.
- **Why here:** Runs before L223, so on fall-through `vest_start_ms < now_ms < vest_end_ms`, hence `span >= 2` and `left >= 1`; the division at L225 cannot be by zero.
- **Assumes:** nothing beyond the ordering.

```rust
// L223-L225
let span = U256::from(self.vest_end_ms - self.vest_start_ms);
let left = U256::from(self.vest_end_ms - now_ms);
self.vest_amount.saturating_mul(left) / span
```
- **What:** Linear release: locked = amount * time_left / span, rounded down.
- **Assumes:** `vest_amount * left` fits in U256; `saturating_mul` silently caps rather than erroring. `left < 2^64`, so this needs `vest_amount < 2^192`, which VARA amounts satisfy; nothing in the file bounds `vest_amount` explicitly (it grows by `fund` at L400, checked_add only).
- **Establishes:** result `<= vest_amount` (since `left < span`), and rounding down means slightly less is locked (slightly more is distributable) than the exact linear amount.
- **Depended on by:** `distributable` (L230), `fund` (L398, which folds this value into the next tranche), `info` (L513, L528-L529).

**Cross-Function Dependencies:**
- No callees.
- Callers: `distributable` L230, `fund` L398, `info` L513. `fund` relies on `remaining == 0` meaning "tranche finished or none" (L401), which is what L221 returns.
- Shared state: `fund` writes the three vest fields (L408-L410); nothing else does.
- Invariant couplings: `reserve >= fees_accrued + unbonding_total + locked_rewards(now)` is what keeps `distributable` from saturating at 0 (L230). `fund` preserves it at the funding instant because L222 makes `locked_rewards(now) == vest_amount == remaining + assets` right after L408-L411.

**Open Questions:**
- unclear; need to confirm Gear's block timestamp is non-decreasing across blocks (`gcore::exec::block_timestamp`, syscalls.rs L64-L66) — the non-decreasing rate argument in `rate_at` rests on it.
