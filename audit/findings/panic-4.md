# panic-4: Pool payouts are fire-and-forget zero-gas value messages; a bounced payout leaves the VARA stranded in the program while the shares stay burned

- **Location**: `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:672-674` (`pay_out`: `send_bytes_with_gas(to, [], 0, as_value(assets))`), used by `Pool::unstake` `:710`, `Pool::claim` `:746`, `Pool::collect_fees` `:844`
- **Bug class**: error-handling: the only error observed is the *send* error; the delivery outcome (error reply carrying the value back) is never handled, and `reserve` is not re-credited
- **Severity**: Medium
- **Confidence**: Medium (depends on the Gear rule that a failed dispatch returns the attached value to the source with an error reply; that is documented behaviour for program destinations, the below-ED user case is less certain)

## Description

`pay_out` returns `Ok` as soon as `msg::send_bytes_with_gas` accepts the message. The
state transition (`unstake` burns shares and does `reserve -= p.net`; `take_unbond` removes
the entry and does `reserve -= entry.assets`) is committed on that `Ok`. Nothing later
reconciles what actually happened to the message:

- If `to` is a **program** (a contract that staked and now unstakes/claims, or a session
  owner that is a program), a message with `gas_limit = 0` cannot execute; the runtime
  fails the dispatch and returns the value to the source, i.e. back to the pool program's
  balance, as an error reply. Sails' `handle_reply` ignores that reply, `reserve` stays
  reduced, the caller's shares (or unbond entry) are gone. The VARA now sits in the
  program's free balance, invisible to `reserve`/`distributable`, with no rescue or
  re-sync entry point. It is unrecoverable by anyone.
- For a **user** destination that does not exist yet and a value below the existential
  deposit, the outcome depends on the runtime's value-transfer path; the same
  "accounting says paid, chain says not" gap applies if the transfer is refused.

The doc comment on `pay_out` only reasons about the mailbox threshold for user accounts;
program destinations are not considered.

## Concrete trigger

1. Contract `C` (any Sails program) calls `Pool::stake` with 100 VARA and receives kVARA.
2. `C` calls `Pool::unstake(shares)`.
3. `pay_out(C, net)` enqueues a 0-gas message; `reserve -= net`, shares burned, `Ok` replied.
4. The message to `C` fails for lack of gas; value returns to the pool; nothing updates
   `reserve`. `C` has no shares and no VARA. The VARA is orphaned in the pool's balance.

## Impact

Loss of the payout for any program-shaped integrator (or a session owner that is a
program); funds are permanently stranded in the pool because `reserve` is internal
accounting and there is no admin sweep of surplus balance. Other holders are not harmed.

## Suggested fix

Either reject program destinations for payouts (check `Syscall::exists`/program-id
predicate is not available on-chain; instead require payouts to be pulled by the recipient
via a `withdraw` step that uses `CommandReply::with_value` so the value rides the reply of
the caller's own message), or make the payout a proper async call with a reply that
re-credits `reserve` and restores the position on error. At minimum add an admin
`sync_reserve`/rescue that moves `balance - reserve` surplus back into `fees_accrued`
so bounced value is not lost forever. (Cross-cluster: also relevant to the async/logic
workers.)

> Overlap: the below-ED user sub-case is filed in detail as `arith-3.md`; this finding adds the program-destination bounce and the missing reserve reconciliation.
