## `PoolState::distributable` in programs/vara-pool/app/src/lib.rs (L229-L231)

**Purpose:** The VARA that belongs to share holders now: reserve minus fees, minus VARA locked for pending unbonds, minus rewards still vesting. It is the numerator of the rate (L241) and the payout ceiling for `unstake` (L377-L378) and `request_unbond` (L421-L422).

**Inputs & Assumptions:**
- `now_ms` (u64): clock, forwarded to `locked_rewards` (L230).
- Implicit state read: `reserve`, `fees_accrued`, `unbonding_total`, plus the vest fields through `locked_rewards`.
- Preconditions:
  - `reserve >= fees_accrued + unbonding_total + locked_rewards(now_ms)` (call it the reserve-cover invariant). Nothing checks it here; the three `saturating_sub`s (L230) clamp at zero instead of failing. What establishes it, path by path:
    - constructor: all zero (L202-L207).
    - `stake`: `reserve += assets` (L365), nothing else grows; preserved.
    - `fund`: `reserve += assets` (L411) and `locked` becomes `remaining + assets` (L408, L222); preserved exactly at the funding instant.
    - `unstake`: requires `p.assets <= distributable` (L378), then `fees += fee`, `reserve -= net`, with `fee + net == assets` (L276, L380-L381); preserved.
    - `request_unbond`: requires `assets <= distributable` (L422), `unbonding += assets` (L424); preserved.
    - `take_unbond`: `reserve -= assets`, `unbonding -= assets` (L441-L442), guarded by `assets <= reserve` (L437); preserved.
    - `collect_fees` handler: `reserve -= fees`, `fees = 0` (L840-L841), guarded by `fees <= reserve` (L839); preserved.
    - `sweep_orphans`: `fees += distributable` when no shares (L351-L352); afterwards `distributable == 0`; preserved.
    - `revert_unstake` / `restore_unbond` / `collect_fees` restore branch: exact inverses with saturating ops (L388-L389, L448-L449, L851-L852).
    - Clock moving backwards raises `locked_rewards` (L222) without touching `reserve`: nothing found that prevents this; it is the one path that breaks the invariant.

**Outputs & Effects:**
- Returns `U256`; no writes.

**Block-by-Block:**

```rust
// L230
self.reserve.saturating_sub(self.fees_accrued).saturating_sub(self.unbonding_total).saturating_sub(self.locked_rewards(now_ms))
```
- **What:** Three clamped subtractions in a fixed order: fees, then unbonds, then locked rewards.
- **Why here:** The order only matters when the reserve-cover invariant is broken; then whichever bucket is subtracted last is the one silently truncated. With the invariant intact all three orders agree.
- **Assumes:** reserve-cover invariant (see above).
- **Establishes:** `distributable <= reserve`; hence any amount bounded by `distributable` is also bounded by `reserve`, which is what L381 (`reserve -= p.net`, a panicking `Sub`) relies on.
- **Depended on by:** `empty` L235, `rate_at` L241, `shares_for` L249, `assets_for` L257, `total_assets` L261, `apy_bps` L266, `sweep_orphans` L351, `unstake` L377, `request_unbond` L421, `info` L527.

**Cross-Function Dependencies:**
- Callee `locked_rewards` (internal, L220-L226): depended on to return a value `<= vest_amount` on every path (it does: L221 zero, L222 `vest_amount`, L225 `amount*left/span` with `left < span`).
- Callers listed above. `unstake` and `request_unbond` treat the return as the hard payout ceiling; `take_unbond` deliberately does not (it checks `reserve`, L437) because unbond assets sit inside `unbonding_total`.
- Shared state: `reserve` is written by `stake` L365, `unstake` L381, `revert_unstake` L389, `fund` L411, `take_unbond` L442, `restore_unbond` L449, `collect_fees` L841/L852. `fees_accrued` by `sweep_orphans` L352, `unstake` L380, `revert_unstake` L388, `collect_fees` L840/L851. `unbonding_total` by `request_unbond` L424, `take_unbond` L441, `restore_unbond` L448.
- Invariant couplings: `reserve` is internal accounting (comment L166-L167): value that reaches the program other than through `stake`/`fund_rewards` is not counted, so the program's real balance is `>= reserve` only if nothing else drains it; see open questions.

**Open Questions:**
- unclear; need to inspect whether anything on Gear/Vara can reduce a program's balance without a program-initiated send (rent, ED reclamation, failed-payout value returns are additive, not subtractive). The reserve-cover invariant is about internal counters; solvency additionally needs `program balance >= reserve`.
