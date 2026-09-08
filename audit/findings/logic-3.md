# logic-3: Vault `settle_deposit` rounds shares *up* to one (`.max(U256::one())`), so a deposit whose price moved during the await is minted a share worth more than it paid, and the rate can decrease

**Location**
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:390` (`let shares = self.shares_for(assets, now_ms)?.max(U256::one());` in `settle_deposit`), versus `:382-383` (`check_deposit` rejects `shares == 0` before the await) and `:738-757` (`Vault::deposit`: check at `:744`, `pull_underlying(...).await` at `:747`, settle at `:751` with a fresh `now_ms`).

**Bug class**: Logic / rounding direction (share issuance rounds in the depositor's favour on one path only).

**Severity**: Low (needs `distributable / total_shares > assets >= MIN_DEPOSIT`, i.e. a per-share price above 1000 base units, which is only reachable after the inflation path in logic-1 or with an extreme `initial_rate`; then it is a small transfer from existing holders to the depositor).

**Description**
The pre-await check and the post-await settlement use different rounding rules. `check_deposit` computes `shares_for(assets, t1)` (floor) and refuses `0`. After the token pull, `settle_deposit` recomputes `shares_for(assets, t2)` and, instead of refusing `0`, mints **one share**. Between `t1` and `t2` the price can only rise (vesting releases `locked_rewards`; an interleaved `redeem` / `request_unbond` adds rounding dust to `distributable`; if every holder left in between, `sweep_orphans` runs and the pool re-prices at `base_rate`). So the only situations where the `.max(1)` fires are exactly the ones where the honest floor is `0`, and the depositor then receives a share worth `distributable / total_shares > assets`.

Numerically, with `S = distributable`, `T = total_shares`, and `a < S / T`: the new rate is `(S + a) / (T + 1) < S / T`, i.e. the rate *decreases* and the difference `S/T - a` (bounded by one share's value) is paid by existing holders. This contradicts the module contract ("rounded down, in the vault's favour", `:265`, `:273`) and the "rate never decreases" property that `burn_shares` / `base_rate` rely on.

Code path: `deposit(a)` at `t1` with `S1/T1 <= a` passes `check_deposit`; `pull_underlying` awaits; a tranche finishes vesting so `S2/T2 > a` at `t2`; `settle_deposit` -> `shares_for = 0` -> `.max(1)` -> `mint_shares(owner, 1)`; `holdings += a`.

**Concrete trigger**
State after logic-1 step 4 in the vault (`T = 1`, `S = 1e9 + 1`, tranche still vesting its last few units): depositor calls `deposit(1_000_000_005)` when `S = 1_000_000_001` (1 share by floor); the reply from the token lands after the tranche released 1000 more units, `S = 1_000_001_001`; `settle_deposit` mints 1 share worth `(1_000_001_001 + 1_000_000_005) / 2 = 1_000_000_503 > 1_000_000_005`. Existing holders lose ~498 base units.

**Impact**
Small value transfer from holders to a depositor and a rate that can tick downward; more importantly, the settlement path silently accepts what the check path rejects, so `check_deposit` is not the invariant it appears to be.

**Confidence**: high on the mechanism; low on reachability under normal prices.

**Suggested fix**
Mirror `check_deposit` in `settle_deposit`: if `shares_for(assets, now)` is zero after the await, do not mint; either refund the pulled tokens (`push_underlying` back to `owner`, and return `BelowMinimum`) or, if refunding is undesirable, credit the assets to `fees_accrued` / holders and return an explicit error. Never round issuance up.
