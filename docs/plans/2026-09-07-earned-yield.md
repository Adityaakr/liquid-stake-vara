# Earned yield: rate from real assets, rewards vest linearly

Prism decision doc, 2026-09-07. Archetype PLAN (looped) → implement. Stakes: one-way door
(moves money). Fleet: 5 lenses (first-principles, adversary, data-integrity, practitioner,
precedent with web grounding) + 3 skeptics (2× Opus, 1× Sonnet).

## 1. Recommendation

Replace the clock-based exchange rate in both programs with a rate derived from what the pool
actually holds, and make every reward vest linearly over a window before it is distributable.
Concretely: `rate = distributable / total_shares`, where `distributable = reserve − fees_accrued −
unbonding_total − locked_rewards(now)`. `fund_rewards` adds to a single vesting tranche whose
release rate never decreases. The stored `rate`, `apy_bps`, `accrue` and `last_accrual_ms` go
away; `apy_bps` in `info()` becomes a derived figure (the vesting rate annualised over
distributable). The vault stops minting "realised yield" out of thin air and takes rewards through
`fund_rewards(assets)` (transfer_from, anyone may fund), with the same vesting. Two further fixes
ride along because the review found them and they touch the same paths: the vault must move its
holdings accounting before the token `await`, and session keys must accept a binding before it
takes effect.

## 2. Why

- **The symptom the user reported is not present today, but the disease is.** A live round trip
  on the local node (stake 100 VARA, unstake in the next block) returned 99.700003 VARA: the fee
  ate one block of accrual. What is wrong is that `rate_at` (pool `lib.rs:194`, vault `lib.rs:211`)
  never reads `reserve`. Liabilities grow on a timer whether or not rewards were funded, so early
  exiters are paid out of later stakers' principal and the last ones hit `InsufficientReserve`.
  A pool that only distributes what it holds cannot do that.
- **Linear vesting is what production vaults do.** Frax sfrxETH (xERC4626, 7-day cycles),
  Yearn v2/v3 (locked profit degrading over 6 h / 10 days), Beefy (1-day lock). Under it, a
  depositor who leaves after one block gets one block's share of the drip: at 35% APY that is
  ~3e-6 of principal per 3-second block, against a 0.3% fee. The fee covers about three days of
  yield, so any exit shorter than that is net negative for the exiter.
- **Deriving the rate removes a class of bugs.** No checkpoint can be missed, no double rounding
  through a stored scaled rate, no admin-set APY that the reserve cannot back.

## 3. Steelman of the rejected option

*Keep the clock, cap it by solvency* (`rate = min(clock rate, distributable / shares)`). It is a
much smaller change, keeps the published APY exact while the pool is funded, and reward deposits
still cannot be sandwiched because the clock, not the deposit, moves the rate. It is a legitimate
fixed-rate product. It was rejected because it keeps an admin-declared number as the source of
truth: when the reserve falls behind, the cap kicks in silently and holders see a rate that
stops moving with no on-chain explanation, and when the reserve runs ahead the surplus sits
outside the rate forever. The vesting design has one source of truth and one knob (the window).

## 4. Assumptions and falsifiers

- Rewards arrive as VARA sent through `fund_rewards` by an operator or anyone (no validator
  integration yet). If rewards instead arrive by a direct balance transfer, they are invisible
  to `reserve` and the design shows 0% APY until someone calls `fund_rewards`.
- The 7-day window matches a 12-hour era cadence (≈14 eras). If eras were much longer than the
  window, APY would flicker between funding events; shorten or lengthen with `set_config`.
- The 0.3% instant fee stays. Without it the round-trip bound is "one block of drip", still
  small, but the strict "never positive" bound relies on the fee.

## 5. Open questions for the human

- Should the exit fee keep going to the admin (`fees_accrued`) or to remaining holders? Today it
  is admin revenue; routing it to holders would compensate stayers for exits.
- Vesting window: 7 days (matches sfrxETH, unbond period) vs. 1–3 days (tracks real APY more
  tightly). Implemented as a constructor and `set_config` parameter, default 7 days.

## 6. Grounded findings (file:line) and evidence tiers

| # | Claim | Tier |
|---|---|---|
| C1 | Round trip today: 100 VARA → 94.7958 kVARA → 99.700003 VARA (`.local/roundtrip.ts` on the dev node, block ~4200) | verified |
| C1b | Under the new design a receipt collects only the drip for the blocks it was held. A fee'd exit is profitable only if one block's drip exceeds 0.3% of TVL (daily drip above ~86% of TVL with 3s blocks); the unbond exit has no fee but forfeits the whole unbond period. gtests `instant_round_trip_earns_nothing` in both programs. | verified |
| C2 | `rate_at` ignores the reserve: pool `lib.rs:194-199`, vault `lib.rs:211-217` | verified |
| C3 | Session hijack: `create_session` pool `lib.rs:393-402` binds any `key` without its consent; `actor_for` `lib.rs:385-391` then redirects that address's own `stake()` (`lib.rs:577-597`) to the binder; only the owner can revoke (`lib.rs:404`). Vault has the identical code. | verified |
| C4 | Vault redeem burns shares at `lib.rs:664` and only decrements `holdings` at `lib.rs:675`, after two awaits; an interleaved message prices off the inflated rate. Same shape in `claim` (`lib.rs:707-730`) and `collect_fees` (`lib.rs:817-847`). | verified |
| C5 | Plain fold-in of a vesting remainder lets 1-planck funding every block turn linear vesting into exponential decay (63% released at day 7). A "release rate never decreases" rule was refuted 3/3 (ceil rounding still extends; a small later tranche vests almost at once). Fix: Yearn v3 weighted average of time left; unit tests `dust_funding_cannot_stretch_a_running_tranche`. | verified |
| C6 | Empty pool with locked rewards: the next 0.01 VARA stake would own the whole remainder. Fix: sweep distributable into `fees_accrued` when shares hit zero and on the first stake after. | supported |
| C7 | `reserve` is internal accounting; plain transfers to the program do not move the rate, so the ERC-4626 donation attack has no entry point. | verified (`lib.rs:303, 332`) |

Evidence summary: 7 verified, 1 supported, 0 unverified, 0 contradicted.

Skeptic round (2× Opus, 1× Sonnet, each on four claims): C3 stood 3/3. The round-trip bound as
first phrased was refuted 3/3 (the unbond path has no fee; the threshold was misstated) and is
restated as C1b. The first fold-in rule was refuted 3/3 and replaced (C5). The vault claim was
refuted 3/3 on the premise that the shortfall mint survives; it does not, so the claim stands
under the design as built and the objection is recorded here rather than acted on.

> Cross-tier verification reduces instance- and tier-level error correlation but not
> shared-lineage blind spots. Treat cross-tier survival as weaker evidence than grounding.

## 7. Design as implemented

Pool state: `total_shares`, `reserve`, `fees_accrued`, `unbonding_total`, `base_rate`,
`vest_amount`, `vest_start_ms`, `vest_end_ms`, `vesting_period_ms`, `instant_fee_bps`,
`unbond_period_ms`, `paused`, balances, allowances, unbonds, sessions, pending sessions.

- `locked(now) = vest_amount × (end − now) / (end − start)` for `start ≤ now < end`, else 0.
- `distributable(now) = reserve − fees − unbonding − locked(now)` (saturating).
- `rate(now) = distributable × SCALE / total_shares` when both are non-zero, else `base_rate`.
- `shares_for(A) = A × S / D` (floor); `assets_for(s) = s × D / S` (floor); at an empty pool
  both use `base_rate`.
- `stake(A)`: if the pool is empty, sweep `distributable` into `fees_accrued` first, then mint at
  `base_rate`. Otherwise mint `A × S / D`. Then `reserve += A`.
- `unstake(s)`: `assets = s × D / S`, fee, net; refuse if `net > D`; burn; `reserve −= net`;
  `fees += fee`; if shares reach zero, `base_rate = rate before the burn`, sweep the remainder.
- `request_unbond(s)`: lock `assets = s × D / S`; burn; `unbonding_total += assets`. Rate-neutral
  by construction. `take_unbond` unchanged (`reserve −= assets`).
- `fund_rewards(V)`: `remaining = locked(now)`; if nothing is vesting the tranche is `V` over a
  full period; otherwise `time_left = (remaining × (end − now) + V × period) / (remaining + V)`
  (Yearn v3's amount-weighted average), `vest_amount = remaining + V`, `start = now`,
  `end = now + time_left`. `reserve += V`. Dust cannot stretch a running tranche; a small tranche
  after a finished one gets a full period. Vesting period floor: one hour.
- `info()` adds `distributable`, `locked_rewards`, `vesting_ends_at`, `vesting_period_secs`; `apy_bps`
  is `r × YEAR × BPS / distributable` while vesting is active, else 0.
- Sessions: `create_session` stores a pending session; the key sends `accept_session(owner)` to
  activate it; only active sessions redirect. The app's enable flow sends the accepts from the
  key right after the batch lands (no wallet prompt).

Vault: identical, with `holdings` in place of `reserve`, `fund_rewards(assets)` pulling the
underlying via `transfer_from`, and the pending-payout accounting moved before the await.

## Changelog
- Loop round 1 (skeptics): dropped the never-decreasing-rate fold-in for the weighted average;
  raised the vesting floor from a minute to an hour after the gtests showed a one-minute window
  lets one block release 5% of a tranche; restated the round-trip bound.
- Open risk: the simulation (`mockAdapter`) still drips at the published APY forever, which is
  the observable behaviour of a pool funded every era but not of an unfunded one.

## Telemetry
- divergence: 0.55 (evidence 0.70, conclusion 0.32) | threshold 0.30 UNCALIBRATED
- grounding: n/a
- models: draft=fable · skeptics=2x-opus+1x-sonnet (cross-tier; version axis unavailable)
- claims: C1 grounded · C1b grounded · C2 grounded · C3 grounded · C4 grounded · C5 grounded · C6 cross-tier-survived · C7 grounded
- skeptic_pool: 2x-opus + 1x-sonnet (cross-tier; version axis unavailable)
- fleet: 5 lenses + 3 skeptics · token-multiple vs single-pass ≈ n/a
