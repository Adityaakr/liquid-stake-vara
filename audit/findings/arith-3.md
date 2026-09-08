# arith-3: Payouts below the existential deposit are refused by the runtime, so sub-ED positions and sub-ED unbond entries are permanently stuck

**Location**
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:30` (`MIN_STAKE = 10_000_000_000` = 0.01 VARA)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:672-674` (`pay_out`: `send_bytes_with_gas(to, [], 0, as_value(assets))`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:371-383` (`unstake`: no minimum on `shares` / `net`, only `net != 0`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:415-429` (`request_unbond`: no minimum on `assets`, only `assets != 0`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:739-756` (`claim` -> `pay_out` -> `restore_unbond` on error)
- Runtime rule: `gear-core-errors-1.10.0/src/lib.rs:129-132` `MessageError::InsufficientValue = 307`: "In case of non-zero message value must be greater than existential deposit".

**Bug class** Unit/threshold mismatch (LOSSYFROM-adjacent: amounts in planck are valid in the program's arithmetic but not representable as a transferable message value). vara-pool only; the vault pays in VFT tokens, which have no ED.

**Severity** Medium

**Description**
The pool's arithmetic works in planck with a floor of 0.01 VARA per stake, and lets a holder unstake or unbond any non-zero share amount. But Gear refuses to send a message whose value is non-zero and below the existential deposit (error 307). Vara's ED is far above 0.01 VARA (1 VARA on mainnet per the runtime; even the smallest plausible value, 0.1 VARA, is 10x `MIN_STAKE`). So:

1. `unstake(shares)` with `net < ED`: `pay_out` fails, `revert_unstake` restores the position, the caller gets `TransferFailed(...)`. A user whose whole position is worth less than `ED / (1 - fee)` can never instant-exit; they must first stake more.
2. `request_unbond(shares)` with `assets < ED`: succeeds (shares burned, `assets` moved into `unbonding_total`). Then every `claim(id)` fails at `pay_out`, `restore_unbond` puts the entry back, and the entry is stuck for good: there is no way to merge, top up, or cancel an unbond entry. The VARA is neither claimable nor distributable.
3. Partial exits: a user with a large position who unstakes/unbonds a slice worth less than ED hits the same wall. For unbond this is a one-way trap per entry.

The `stake` error path (`CommandReply::with_value(value)` for a rejected sub-`MIN_STAKE` stake) is not affected in practice: a reply carrying value below ED also fails, sails panics, and Gear's failed-execution handling returns the attached value to the sender anyway.

**Concrete numbers** (ED taken as 1 VARA = `1e12` planck; scale accordingly if the deployed runtime differs)
- Alice stakes 0.5 VARA (`5e11` planck, allowed since `>= MIN_STAKE = 1e10`). Fee 30 bps. `unstake(all)`: `assets = 5e11`, `fee = 5e11*30/10_000 = 1.5e9`, `net = 4.985e11 < 1e12` -> `send_bytes_with_gas` returns `InsufficientValue` -> `TransferFailed`. Alice must stake at least `1e12/(1-0.003) - 5e11 = 5.03e11` more before she can leave.
- Bob holds 1,000 VARA and calls `request_unbond` for shares worth 0.9 VARA (`9e11`): entry created, `unbonding_total += 9e11`. After 7 days `claim` -> `pay_out(9e11)` fails -> entry restored. Repeats forever; the 0.9 VARA is locked and also excluded from the rate for everyone else.
- Lower bound check: with `MIN_STAKE = 1e10` the gap to a 1 VARA ED is 100x, to a 0.1 VARA ED still 10x, so the trap exists for any ED above 0.01 VARA.

**Impact**
Permanent loss of access for any unbond entry below ED (user-triggered, no admin recovery), and inability to exit for positions below ED. Amounts are small per incident but the failure is silent at request time and irreversible; the UI's `preview_unstake` shows a successful preview because the program's arithmetic has no notion of the ED floor.

**Confidence** Medium-High. The runtime rule and the code paths are verified in source; the exact ED constant of the target runtime was not verified locally (it is well above `MIN_STAKE` under every published Vara value).

**Suggested fix**
Enforce a payout floor in the arithmetic layer: reject `unstake` when `net < MIN_PAYOUT` and `request_unbond` when `assets < MIN_PAYOUT`, with `MIN_PAYOUT` set at or above the chain's ED (make it an admin-configurable constant, since ED can change with runtime upgrades), and raise `MIN_STAKE` to at least `MIN_PAYOUT / (1 - MAX_FEE)` so a fresh stake is always exitable. Alternatively let `claim` sweep several matured entries in one payout so small entries can be combined above ED.
