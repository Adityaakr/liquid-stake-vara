## `VaultState::locked_rewards` in programs/vault/app/src/lib.rs (L241-L247)

**Purpose:** Returns the part of the current rewards tranche that has not yet vested at `now_ms`. It is the "rewards still vesting" term of `distributable` (L251), so it decides how much of the vault's holdings the share price may see. Without it every funded reward would jump the rate on arrival (the doc comment at L10-L12 states the vesting intent).

**Inputs & Assumptions:**
- `now_ms` (u64): block timestamp in milliseconds. Trust: trusted for on-chain callers (always `Syscall::block_timestamp()` via `now_ms()` L704-L706); in unit tests an arbitrary value (L1073 passes `T0 - 5`).
- Implicit state read: `vest_amount`, `vest_start_ms`, `vest_end_ms` (L193-L195).
- Precondition: `vest_start_ms <= vest_end_ms` whenever `vest_amount != 0`. Established by `fund` L436-L437 (`vest_end_ms = now_ms.saturating_add(time_left)` with `vest_start_ms = now_ms`). The constructor sets all three to zero (L226-L228).
- Precondition: `vest_end_ms - vest_start_ms > 0` on the path that divides (L246). Established by `fund` L434 (`time_left.max(U256::one())`) plus L437, except when `now_ms.saturating_add(time_left)` saturates at `u64::MAX` with `now_ms == u64::MAX`; nothing found for that edge. In practice `now_ms` is a millisecond timestamp far below `u64::MAX`.

**Outputs & Effects:**
- Returns a `U256` in `[0, vest_amount]`. No state writes, no messages.
- Postcondition: zero when nothing is vesting or the tranche has ended; the full amount at or before the start; a linear interpolation in between, rounded down (L246 integer division), so the *released* part is rounded up.

**Block-by-Block:**

```rust
// L242
if self.vest_amount.is_zero() || now_ms >= self.vest_end_ms { return U256::zero(); }
```
- **What:** Nothing vesting, or the tranche is over.
- **Why here:** Guards the subtraction at L245 (`vest_end_ms - now_ms` needs `now_ms < vest_end_ms`).
- **Assumes:** nothing beyond the fields.
- **Establishes:** `now_ms < vest_end_ms` and `vest_amount > 0` for the rest.
- **Depended on by:** L245.

```rust
// L243
if now_ms <= self.vest_start_ms { return self.vest_amount; }
```
- **What:** Before (or exactly at) the start of the tranche the whole amount is locked.
- **Why here:** Makes the `<=` case explicit and keeps `left <= span` below (with `now_ms > vest_start_ms`, `vest_end_ms - now_ms < vest_end_ms - vest_start_ms`).
- **Assumes:** a clock that moves backwards relative to `vest_start_ms` should lock everything (test L1073 exercises `T0 - 5` on a vault that never funded, so this branch is not covered there).
- **Establishes:** `vest_start_ms < now_ms < vest_end_ms` below, hence `span >= 1` and `left < span`.
- **Depended on by:** L244-L246.

```rust
// L244-L246
let span = U256::from(self.vest_end_ms - self.vest_start_ms);
let left = U256::from(self.vest_end_ms - now_ms);
self.vest_amount.saturating_mul(left) / span
```
- **What:** Linear interpolation of the remaining fraction.
- **Why here:** After both guards the two `u64` subtractions cannot underflow.
- **Assumes:** `span != 0` (see preconditions); `vest_amount * left` fits — `saturating_mul` caps it, which would understate `locked` only if `vest_amount > U256::MAX / left`; nothing bounds `vest_amount` except the underlying token's supply arithmetic (demo token `credit` L85 `checked_add`).
- **Establishes:** result `<= vest_amount` because `left < span`.
- **Depended on by:** `distributable` L251, `fund` L425 (folds the still-locked remainder into the next tranche), `info` L555.

**Cross-Function Dependencies:**
- Callee: none (pure arithmetic).
- Callers: `distributable` L251, `fund` L425, `info` L555. `distributable` relies on the result being `<= vest_amount` and on `vest_amount` being physically inside `holdings`; that second property is established by `fund` L438 adding `assets` to `holdings` in the same transition that adds them to `vest_amount` L435, and by `fund_rewards` only calling `fund` after `pull_underlying` succeeded (L834-L837).
- Shared state: `vest_amount`/`vest_start_ms`/`vest_end_ms` written only by `fund` L435-L437 and the constructor L226-L228.
- Invariant couplings: the accounting bound `holdings >= fees_accrued + unbonding_total + locked_rewards(now)` (see `vault-distributable.md`) uses this function's monotone decrease in time: `locked_rewards` never increases between two calls at increasing `now_ms` unless `fund` runs in between.

**Open Questions:**
- unclear; need to inspect whether Gear's `block_timestamp` can be non-monotone across blocks on Vara. If it can, L243 turns a full tranche back to "all locked" for one call, which moves `distributable` down and then up again.
