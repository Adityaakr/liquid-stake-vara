## `PoolState::sweep_orphans` in programs/vara-pool/app/src/lib.rs (L349-L354)

**Purpose:** When no shares exist, whatever is distributable (rewards that vested during the empty spell, rounding dust left by the last exit) is moved to the fee bucket so the next staker cannot claim it (comment L347-L348). It is what makes `shares_for`'s empty-path pricing at `base_rate` consistent: after the sweep `distributable == 0`, so `empty()` is true for the right reason.

**Inputs & Assumptions:**
- `now_ms` (u64): clock, forwarded to `distributable`.
- Implicit state read: `total_shares`, `distributable(now_ms)`.
- Preconditions: none checked. Called only from `stake` L361, after `ensure_live` and the amount checks (L358-L360) and before `shares_for` (L362).

**Outputs & Effects:**
- Writes `fees_accrued += distributable(now_ms)` (L352, saturating) when `total_shares == 0`. Nothing otherwise.
- Postcondition on the write path: `distributable(now_ms) == 0` (reserve-cover invariant intact: `reserve - fees_new - unbonding - locked = 0`).

**Block-by-Block:**

```rust
// L350-L353
if self.total_shares.is_zero() {
    let orphaned = self.distributable(now_ms);
    self.fees_accrued = self.fees_accrued.saturating_add(orphaned);
}
```
- **What:** Fold the distributable amount into fees.
- **Why here:** In `stake` it runs before `shares_for` (L362) so the first stake after an empty spell is priced by `base_rate`, not by `orphaned / 0`. It also runs before `stake`'s remaining error returns (L362 `Overflow`, L363 zero shares), so on those error paths the sweep persists (an `Err` reply is not a trap: sails exposure.rs L109-L137). The persisted write is the same one the next successful stake would perform.
- **Assumes:** "no shares" is the only case where distributable VARA is ownerless. The `total_shares > 0 && distributable == 0` case is not swept (nothing to sweep) and the `total_shares > 0 && distributable > 0` case belongs to holders.
- **Establishes:** `distributable == 0` after the sweep when `total_shares == 0`.
- **Depended on by:** `shares_for` L246-L247 (takes the `base_rate` branch), `stake` L363.

**Cross-Function Dependencies:**
- Callee `distributable` (internal).
- Callers: `stake` L361 only. Not called from `fund` (rewards funded into an empty pool sit in `reserve` and vest; whatever vests before the next stake is swept then), not from `take_unbond`/`claim`, not from `collect_fees`. So an admin calling `collect_fees` during an empty spell collects only fees already swept, not the rewards vested since the last stake.
- Shared state: `fees_accrued` (see `distributable` record for the full writer list).
- Invariant couplings: rewards funded while the pool is empty and still vesting at the next stake are not swept (they are `locked`, hence not distributable, L230) and vest to the new holders afterwards (test L1090-L1095).

**Open Questions:**
- none.
