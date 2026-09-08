# logic-4: Pause is applied inconsistently: in-flight vault deposits settle after `pause`, `fund_rewards` ignores pause entirely, and matured unbond claims are blocked by it

**Location**
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:378-385` (`check_deposit` calls `ensure_live` before the await) vs `:388-394` (`settle_deposit` after the await does not), reached from `Vault::deposit` `:738-757`.
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:423-440` + `:826-842` and `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:396-413` + `:760-776` (`fund` / `fund_rewards`: no `ensure_live`).
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:432-444` and `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:459-471` (`take_unbond` calls `ensure_live`, so `claim` of an already-matured, already-priced unbond is refused while paused), `:313-318` / `:334-339` (`approve_shares` also paused).

**Bug class**: Logic / access-control state machine (pause guard placement).

**Severity**: Low (admin is trusted; this is about what `pause` actually guarantees).

**Description**
1. `Vault::deposit` checks `paused` only in `check_deposit`, before `pull_underlying(...).await`. Gear persists state at the await; if the admin's `pause` executes between the check and the token reply, `settle_deposit` still runs `sweep_orphans`, `shares_for`, `mint_shares` and `holdings += assets`. New shares are therefore minted while the vault is paused. If `pause` was used because the price/tranche was wrong, those shares are priced under the wrong state.
2. `fund` / `fund_rewards` never consult `paused` in either program. While paused, anyone can keep adding to `reserve` / `holdings` and rewrite the vesting tranche (`vest_amount`, `vest_start_ms`, `vest_end_ms`), moving the rate that is supposed to be frozen.
3. Conversely `take_unbond` refuses matured unbonds (`Paused`) even though their `assets` were fixed at request time and already removed from `distributable`; a pause therefore withholds funds the pool has already promised, and `approve_shares` (a pure bookkeeping change) is paused too.

Code path for 1: `deposit(a)`: `check_deposit` ok -> await -> `pause()` from admin (sync, completes) -> token reply -> `settle_deposit` (no `ensure_live`) -> shares minted. For 2: `pause()` then `fund_rewards{value}` -> `fund` -> `Ok`.

**Concrete trigger**
Vault: user sends `deposit(1_000_000)`; admin sends `pause()` in the same block; the demo token reply arrives next block; `balance_of(user)` increases while `is_paused()` is true. Pool: `pause()`, then `fund_rewards{value: 1e12}` from any account succeeds and `info().vesting_ends_at` moves.

**Impact**
`pause` does not freeze share issuance or the rate, and does block payouts that are no longer rate-dependent. Operationally confusing at best; if `pause` is the incident response for a mis-priced tranche, it is not sufficient.

**Confidence**: high

**Suggested fix**
Re-check `ensure_live` in `settle_deposit` (and refund on failure), add `ensure_live` to `fund`, and consider exempting `take_unbond` (and `approve_shares`) from the pause, or document that pause blocks claims.
