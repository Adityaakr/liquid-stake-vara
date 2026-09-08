# Logic-correctness cluster: notes, weaker suspicions, and cleared checks

Scope: `programs/vara-pool/app/src/lib.rs` (P), `programs/vault/app/src/lib.rs` (V), `programs/demo-token/app/src/lib.rs` (T). Cluster finders applied: ord-eq-hash, adversarial-trait, closure-panic, float-edge, string-comparison, serialize-struct-mismatch, nondeterminism, collection-key-mutation, plus arithmetic-overflow / assertion-reachable / toctou as sanity passes. Filed: logic-1..4.

## Cluster finders with no hits (checked, not applicable)
- ORDEQHASH: no hand `Ord/PartialOrd/Eq/PartialEq/Hash` impls; all derived. `Unbond` sort key is `id` (unique, monotonic).
- TRAITADV / CLOSUREPANIC: no `unsafe`, no generic trait-bounded APIs.
- FLOATEDGE: no `f32`/`f64`.
- STRCMP: no string comparisons gate anything.
- SERFIELDS: no manual `Serialize`; SCALE codecs are derived via `sails_type`.
- NONDET: only `BTreeMap`/`BTreeSet`; iteration (`minters()`) is ordered.
- KEYMUT: keys are `ActorId` / `(ActorId, ActorId)` / `u64`, no interior mutability.
- TOCTOU (filesystem): n/a. The async check/settle drift in V is covered in logic-3 / logic-4.

## Already filed by other clusters (not duplicated here)
- Constructor `instant_fee_bps` unvalidated -> `assets - fee` underflow panic: arith-1 / panic-1.
- `unbond_period_secs` unbounded, `claimable_at` saturates: arith-2.
- Payouts below the existential deposit: arith-3.
- `revert_redeem` `saturating_sub` after an interleaved `collect_fees` (holders pay a reverted fee): panic-2.
- `pay_out` zero-gas value message bounces for program recipients, VARA stranded: panic-4.

## Invariants verified (no finding)
- `reserve/holdings >= fees_accrued + unbonding_total + locked_rewards` holds across every transition: `stake`/`settle_deposit` (+a to both sides, sweep first when `T == 0`), `unstake`/`begin_redeem` (requires `assets <= distributable`, removes `net` from reserve and adds `fee` to fees: distributable falls by exactly `assets`), `request_unbond` (requires `assets <= distributable`), `take_unbond` (reserve and unbonding drop together), `fund` (reserve and locked rise together), `collect_fees`/`take_fees` (reserve and fees drop together), and all compensation paths are exact inverses except the one in panic-2. `locked_rewards` is non-increasing within a tranche and equals `remaining` at the instant of a fold-in, so `distributable` never saturates to zero from below. Consequence: the `saturating_sub` chain in `distributable` never masks anything today, and `take_unbond`'s `assets > reserve` and `collect_fees`'s `amount > reserve` checks are unreachable.
- `distributable == 0 && total_shares > 0` is unreachable: `assets_for(shares)` with `shares < T` is strictly less than `distributable` (floor of `shares*S/T`), so an exit can only zero `distributable` by burning every share. Hence `empty()` is only true for `T == 0`, and the `base_rate` branch is never used to price a non-empty pool.
- Division safety: `/ base_rate` (>= SCALE, never rewritten to zero since `rate_at` is non-decreasing from >= SCALE in P; in V it can dip via logic-3 but stays > 0), `/ distributable` (non-empty branch only), `/ total_shares` (non-empty), `/ span` (`time_left.max(1)`), `/ total` in `fund` (`assets > 0`), `/ (span * d)` in `apy_bps` (both > 0).
- Rounding direction is in the pool's favour on all P paths and all V paths except `settle_deposit` (logic-3). Fee rounds down (user's favour by < 1 base unit).
- Rate monotonicity: `stake`, `unstake`, `request_unbond` all give `rate' >= rate`; `burn_shares` snapshots the rate before the last burn; `sweep_orphans` re-anchors at `base_rate`. Only logic-3 can lower it.
- Allowance semantics: `U256::MAX` is infinite (not decremented), `spender == from` skips the allowance, zero approve deletes the entry, `spend_allowance` then `transfer_shares` failure rolls back via the `unwrap_result` trap in the sync `Vft` service. Standard; no bypass found.
- Session state machine: the key must call `accept_session`, so nobody can bind a foreign address; a key mapped to owner A cannot be accepted or proposed for owner B; replacing a session removes the old key's mapping; `revoke_session` clears both pending and active. Circular sessions (A is B's key and B is A's key) are possible but need both parties' `accept`. Only lifecycle/expiry handling is off (logic-2).
- `fund` fold-in arithmetic: `total = remaining + assets`, `time_left = (remaining*left + assets*period)/total` is between `left` and `period`; `low_u64()` is exact since both bounds fit u64; `vest_start_ms = now` and `locked_rewards(now) == total`, so `distributable` is unchanged by a fold-in. Dust cannot stretch a running tranche (tested).

## Weaker suspicions (not filed)
1. **Permissionless `fund_rewards` reshapes the vesting schedule of rewards that are not the funder's.** `fund` P:396 / V:423 amount-weights the remaining time with a full period, so a large top-up shortly before an old tranche ends pushes the old tranche's remaining amount out to ~a full period, and a large top-up after `set_config` shortened the period pulls it in. The funder pays for it (their amount vests to holders), so it is a schedule-shaping tool rather than theft. Consider admin-only funding (also recommended in logic-1).
2. **Rewards funded while `total_shares == 0`.** They vest to nobody; the vested part is swept into `fees_accrued` on the next `stake` (`sweep_orphans` P:349 / V:370, i.e. to the admin), and the still-locked part vests to whoever stakes next, even a `MIN_STAKE` staker. Documented design, but a funder can lose everything to the admin and a 0.01 VARA staker can inherit a large tranche. Worth a guard `fund` -> `Err` when `total_shares == 0`.
3. **`initial_rate` has no upper bound** (P:198, V:219). A deployer typo (e.g. rate in 1e30) makes `shares_for` return 0 for every stake (`BelowMinimum`) and there is no way to change `base_rate` while `T == 0`; `assets_for` would return `Overflow` for large share counts. Deployer error only.
4. **Unbounded per-owner vectors.** `unbonds: BTreeMap<ActorId, Vec<Unbond>>` grows by one per `request_unbond` (min 1 share, min `assets == 1` base unit); `take_unbond` is O(n) and `restore_unbond` sorts. A session key with only `Unbond` rights can inflate its owner's list until the owner's `claim` runs out of gas. `Session.actions` is also unbounded (duplicates allowed). Self/keyholder griefing; cap the list length and dedupe actions.
5. **Fee rounds to zero for `assets < BPS / fee_bps`** (e.g. `< 334` base units at 30 bps). `unstake` allows any `shares >= 1`, so the fee is avoidable by dust-sized exits, but each costs a message of gas, far above the saving. Not economical.
6. **Session key with `Deposit` + `Redeem` (V) or `Stake` + `Unstake` (P) can grief the owner** by cycling the owner's funds through the instant fee (fee goes to the admin) or, in V, by pulling the owner's whole token allowance into the vault. Inherent to delegated rights; owners should scope `actions` and approvals.
7. **Demo token: a minter may `burn` any account's balance without allowance** (T:302-312, `ensure_minter` only). The docs say the minter role exists "so the vault can mint yield"; the vault never calls `burn`, so the burn right is wider than needed. Admin-granted role; Info.
8. **Demo token faucet makes vault economics moot**: anyone can mint unlimited underlying and `fund_rewards` with it, so the vault's rate is freely manipulable (used to full effect in logic-1). Acceptable for a demo, fatal if the vault is ever pointed at a real token without changing `fund_rewards` access.
9. **V trusts `assets` == amount actually received** (`settle_deposit(owner, assets)` after `transfer_from(.., assets)`): correct for the demo token; would misaccount for a fee-on-transfer or rebasing underlying.
10. **No deposit slippage protection**: `check_deposit` previews shares at `t1`, `settle_deposit` mints at `t2`; the depositor may get fewer shares than `preview_deposit` showed (rate only rises). Standard for 4626 without `minShares`; add an optional `min_shares` argument.
11. **`transfer_admin` is single-step** in all three programs; a typo hands the admin role (and fee stream) to an unrecoverable address.
12. **`info()` while `T == 0`** reports the orphan amount as `distributable` with `total_assets == 0`; harmless but confusing for a UI.
13. **`last_claim` (T) and `pending_sessions` (P/V) grow without bound** and are never pruned; state-size cost only.
14. Sandwiching `fund_rewards` is not profitable: rewards vest over >= 1 h so a one-block round trip earns at most `3 s / 3600 s` of the tranche pro rata and pays the instant fee (or waits the unbond period). Confirmed by the existing tests.
