# gear-4: A sub-ED payout to an account that no longer exists is silently dropped by the runtime, while the pool books it as paid (also a correction to arith-3: 1.10 does not refuse sub-ED value at send time)

**Location**
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:672-674` (`pay_out` returns `Ok` on send-syscall success)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:30` (`MIN_STAKE = 0.01 VARA`), `:371-383` (`unstake` allows any `net > 0`), `:415-429` / `:432-444` (`request_unbond` / `take_unbond` allow any `assets > 0`)
- Related: `/Users/adityakrx/liquid-stake/audit/findings/arith-3.md` (assumes `MessageError::InsufficientValue` is enforced)

**Bug class** Execution model: value delivery semantics (existential deposit on the *receiving* side) not modelled; send success mistaken for delivery.

**Severity** Low

**Description**

The 1.10 core-processor no longer checks an outgoing message's value against the existential deposit. `send_commit` runs `check_forbidden_destination`, `charge_expiring_resources` (which is `reduce_gas` + `charge_message_value`, i.e. only the value counter), `charge_sending_fee`, `charge_for_dispatch_stash_hold` (`core-processor-1.10.0/src/ext.rs:976-990, 452-456, 421-427`); the only place `existential_deposit` is consulted is `create_program` (`ext.rs:1368-1373`). `gear-core-errors-1.10.0/src/lib.rs:129-132` still defines `InsufficientValue = 307`, but nothing in `core-processor-1.10.0`, `gear-core-1.10.0`, `gear-core-backend-1.10.0` or `gtest-1.10.0` raises it (grep). So arith-3's premise that a sub-ED `unstake`/`claim` fails with `TransferFailed` and restores the position is not what 1.10 does: the send succeeds and `pay_out` returns `Ok`.

What actually happens is decided when the value is delivered. The bank transfer checks the *receiver*: `gtest-1.10.0/src/state/bank.rs:109-127` `transfer_value` -> `if !Accounts::can_deposit(to, value) { /* unused value will be lost */ return; }` with `can_deposit = balance(to) + value >= EXISTENTIAL_DEPOSIT` (`src/state/accounts.rs:186-188`), and `EXISTENTIAL_DEPOSIT = UNITS = 1e12` (`src/lib.rs:552-555`, 1 VARA, matching "~1 VARA on mainnet" in `references/gear-execution-model.md`). Consequences:

- Recipient account exists (balance >= ED): a payout below 1 VARA is delivered normally. Most of arith-3's scenarios therefore work.
- Recipient account does not exist (balance 0, e.g. the owner emptied and reaped the account after staking, or a session owner that never held VARA): a payout below 1 VARA is dropped by the bank. The pool has already burned the shares / removed the unbond entry and decremented `reserve` (`lib.rs:379-381`, `:441-442`), emitted the event, and returned `Ok`. The VARA is neither on the user nor in `reserve`; in gtest it is simply lost, on-chain it accrues to gear-bank's unused-value bucket (UNVERIFIED: pallet-gear-bank sources are not vendored; gtest's comment mirrors that behaviour).

**Trigger** Owner account with zero balance at claim time plus a payout `< 1 VARA` (fees or a partial exit make this easy: `unstake` of a 0.5 VARA slice; an unbond entry of 0.9 VARA).

**Impact** Loss of that payout for the user (bounded by ED per event); no effect on other holders. Combined with the missing recipient parameter (gear-2) there is no way to route the payout elsewhere.

**Confidence** High that no send-time ED check exists in 1.10 (vendored sources); Medium that the on-chain delivery path drops rather than fails (mirrored by gtest, pallet not read).

**Suggested fix** Keep payouts atomic with delivery by enforcing a program-side floor: refuse `unstake`/`request_unbond` when `net`/`assets < ED_FLOOR` (1 VARA) unless the caller's whole position is being closed *and* the caller passes a recipient it controls; or, simpler, require `net >= 1 VARA` and let dust accumulate. Update arith-3 to describe the drop-on-empty-account behaviour rather than a runtime refusal.
