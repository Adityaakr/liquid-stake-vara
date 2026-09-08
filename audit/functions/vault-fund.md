## `VaultState::fund` in programs/vault/app/src/lib.rs (L423-L440)

**Purpose:** Post-await settlement of `fund_rewards`: count arrived reward tokens in `holdings` and start (or extend) the vesting tranche. The still-locked remainder of the previous tranche is folded into the new one and the new length is the amount-weighted average of the time left and a full period (comment L419-L422, tests L1152-L1168).

**Inputs & Assumptions:**
- `assets` (U256): amount the token confirmed. Trust: trusted under the same condition as `settle_deposit` (the `Ok(Ok(true))` arm).
- `now_ms` (u64): post-await timestamp (L837).
- Implicit state read: `vest_*`, `vesting_period_ms`, `holdings`.
- Precondition: tokens arrived. Established by `fund_rewards` L834 `?`.
- Precondition: `vesting_period_ms >= MIN_VESTING_SECS * 1000`. Established by the constructor L222 (`clamp`) and `set_config` L506.
- Not checked: `paused` (no `ensure_live` here or in `fund_rewards`), `total_shares > 0` (funding an empty vault is allowed; the vested part is then swept to fees by the next deposit, L370-L375).

**Outputs & Effects:**
- State writes: `vest_amount` L435, `vest_start_ms` L436, `vest_end_ms` L437, `holdings` L438.
- Returns `Err(ZeroAmount)` L424 (before any write), `Err(Overflow)` L427 (before any write) or L438 (**after** L435-L437 have been written), else `Ok(())`.
- Partial write: on the L438 overflow the tranche fields describe rewards that `holdings` does not count; `distributable` then subtracts `locked_rewards` from a `holdings` that never grew. `fund_rewards` returns the error at L837 with the tokens already pulled.

**Block-by-Block:**

```rust
// L424-L427
if assets.is_zero() { return Err(VaultError::ZeroAmount); }
let remaining = self.locked_rewards(now_ms);
let period = U256::from(self.vesting_period_ms);
let total = remaining.checked_add(assets).ok_or(VaultError::Overflow)?;
```
- **What:** Zero gate; snapshot what is still locked; the new tranche total.
- **Why here:** `remaining` must be read before `vest_*` are overwritten.
- **Assumes:** `fund_rewards` already rejected zero (L831), so L424 is a second line.
- **Establishes:** `total >= assets > 0`, so the division at L432 is safe.
- **Depended on by:** L428-L437.

```rust
// L428-L434
let time_left = if remaining.is_zero() {
    period
} else {
    let left = U256::from(self.vest_end_ms.saturating_sub(now_ms));
    (remaining.saturating_mul(left).saturating_add(assets.saturating_mul(period))) / total
};
let time_left = time_left.max(U256::one()).low_u64();
```
- **What:** Weighted average of the old tranche's remaining time and a full period.
- **Why here:** A running tranche cannot be stretched by dust (the weight of `assets` is proportional to its size) and a fresh tranche gets a full period.
- **Assumes:** `left <= period`-ish so that the average fits in `u64` — it does: a weighted mean of two `u64` values is `<=` the larger, so `low_u64()` is lossless. When `remaining != 0`, `now_ms < vest_end_ms` (from `locked_rewards` L242), so `left >= 1`.
- **Establishes:** `time_left >= 1` ms.
- **Depended on by:** L437 and hence `locked_rewards`' `span > 0`.

```rust
// L435-L438
self.vest_amount = total;
self.vest_start_ms = now_ms;
self.vest_end_ms = now_ms.saturating_add(time_left);
self.holdings = self.holdings.checked_add(assets).ok_or(VaultError::Overflow)?;
```
- **What:** Write the tranche, then count the tokens.
- **Why here:** `holdings` last (partial-write consequence above).
- **Assumes:** restarting the tranche at `now_ms` with the remainder folded in preserves what holders were owed: `locked_rewards(now_ms)` immediately after equals `total` (L243 path since `now_ms <= vest_start_ms`), i.e. `remaining + assets` — the previously vested part stays vested, the previously locked part stays locked. Established by construction.
- **Establishes:** `holdings` includes `assets`; `distributable` unchanged at `now_ms` (holdings up by `assets`, locked up by `assets`) — the rate does not jump at funding (test L1064, gtest L179-L184).
- **Depended on by:** `RewardsFunded` event L840, all later pricing.

**Cross-Function Dependencies:**
- Callee `locked_rewards` (internal).
- Callers: `Vault::fund_rewards` L837 only.
- Shared state: `vest_*` (only writer besides the constructor); `holdings`.
- Invariant couplings: `holdings >= fees + unbonding + locked` preserved (both sides up by `assets`). No `ensure_live`: funding proceeds while paused. `set_config` can change `vesting_period_ms` between two fundings; the new period applies from the next `fund`.

**Open Questions:**
- none.
