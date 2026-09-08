# gear-2: `pay_out` sends VARA with `gas_limit = 0`; if the recipient is a program the message cannot execute, the value bounces back to the pool's free balance, and the burned position is gone

**Location**
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:670-674` (`pay_out`: `send_bytes_with_gas(to, [], 0, value)`, `Ok(())` as soon as the send syscall succeeds)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:710-720` (`unstake`: burn + `reserve -= net` already applied by `s.unstake`, `pay_out(owner, ..)`, `revert_unstake` only on `Err`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:746-755` (`claim`), `:844-855` (`collect_fees(to)`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:681-687` (`stake`: owner = `Syscall::message_source()` or the session owner; a program can be either)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:166-168` (`reserve` is internal accounting; value that lands on the program outside `stake`/`fund` is invisible)

**Bug class** Execution model: value transfer modelled as fire-and-forget; program-vs-user destination semantics differ; no reconciliation of bounced value.

**Severity** Medium

**Description**

`send_bytes_with_gas(to, [], 0, value)` is accepted by the runtime for any destination: `core-processor-1.10.0/src/ext.rs:461-479` (`get_reducing_gas_limit`: "Zero gasful message is a special case", only `0 < gas < mailbox_threshold` is refused). What happens next depends on `to`:

- `to` is a user account: the message is routed through `send_user_message`; with an explicit `Some(0)` gas limit it does not meet the mailbox threshold, so it is not stored in the mailbox and the value is transferred at once (`gtest-1.10.0/src/manager/send_dispatch.rs:276-330`, the `gas_limit >= threshold` branch is the mailbox path; the explicit `0` never takes it). This is the case the code comment at `lib.rs:670-671` describes, and it is correct.
- `to` is a program: the dispatch is queued to that program with a 0-gas node (`gtest-1.10.0/src/manager/journal.rs:145-183`, `split_with_value(.., 0)`), the value is deposited from the pool into the bank. Precharging the very first operation fails (`core-processor-1.10.0/src/precharge.rs:160-171`, `PreChargeGasLimitExceeded` -> `process_execution_error`). `process_error` then (a) pushes `SendValue { from: pool, to: dest_program, value, locked: true }` and (b) sends a system error reply carrying the same `value` back to the pool (`core-processor-1.10.0/src/processing.rs:283-311, 338-364`; `ReplyPacket::system(payload, value, err)` at `gear-core-1.10.0/src/message/reply.rs:211-218`). That reply is processed at the pool with the failed message's leftover gas (0, `gas_limit: None` -> `split`), so precharge fails again, but for a `Reply` dispatch `process_error` still delivers the value: `to_send_reply = false` and `SendValue { from: dest_program, to: pool, value }` (`processing.rs:297-311`). Either way the VARA ends up back on the pool's free balance.
- `to` is an exited or never-initialised program: `ProcessErrorCase::ProgramExited / Uninitialized` -> `NoExecution` plus the same error reply with the value (`processing.rs:225-240, 384-388`).

Meanwhile the pool has already committed: `s.unstake(..)` burned the shares and reduced `reserve` (`lib.rs:379-381`), `pay_out` returned `Ok(())` because the *send syscall* succeeded, and `Unstaked` / `Claimed` / `FeesCollected` events were emitted. The bounce is asynchronous and lands in a later execution (`handle_reply`, which Sails generates but which does nothing for this non-async program), so nothing updates `reserve`. The VARA is now on the program's balance but outside `reserve`; there is no sweep, `collect_fees` only pays `fees_accrued`, and the program never calls `exit`. It is permanently orphaned and the recipient got nothing.

Who can be a program recipient. `Syscall::message_source()` maps to `gcore::msg::source()` (`references/sails-syscall-mapping.md`, Message Context table) and is the sending program's id when a program sends the message, so a Gear program can `stake` (attaching value the same way), hold kVARA, `unstake`/`request_unbond`/`claim`, and it can also `accept_session` as a key or be the owner behind a session. `collect_fees(to)` lets the admin name any `to`. None of these paths check whether the destination can receive a 0-gas message.

**Trigger** Any Gear program (an integrating protocol, a DAO/treasury program, a smart-contract wallet, a router) stakes VARA and later calls `unstake`, `request_unbond` + `claim`, or is named as `to` in `collect_fees`. Note: Substrate-level multisig/proxy accounts are ordinary accounts and are *not* affected; only Gear programs are.

**Impact** Loss of the program's whole payout (its shares are burned, the unbond entry is removed, the fee bucket is zeroed) with the VARA stuck on the pool's balance forever. The pool's books stay internally consistent (`reserve` decreased, balance did not), so other stakers are not harmed, but the pool silently accumulates unrecoverable VARA.

**Confidence** High. Every hop is in the vendored `core-processor-1.10.0`, `gear-core-1.10.0` and `gtest-1.10.0` sources; the pool has no code path that could re-credit a bounced value. Whether pallet-gear's journal handler matches gtest's for the program-destination branch was not read (pallet sources are not vendored), but gtest is the runtime's reference mirror and core-processor is shared (UNVERIFIED only on that point).

**Suggested fix**
1. Let payouts name a recipient: `unstake(shares, to: Option<ActorId>)`, `claim(id, to)`, so program integrators can direct VARA to a user account, and document that the default recipient must be a user account.
2. Reconcile bounces: implement `handle_reply` in the pool (Sails `#[handle_reply]` on the program, `sails-macros-core-1.0.1/src/program/mod.rs:109-131`) that, on an error reply whose `reply_to` is a payout message id recorded in a small `pending_payouts: BTreeMap<MessageId, (owner, amount)>`, restores the position (`revert_unstake` / `restore_unbond` / fees) instead of leaving the value orphaned. Store the map entry in `pay_out` (it has the message id from the send) and remove it on a success (`Auto`) reply.
3. Add an admin `sweep_untracked()` limited to `Syscall::value_available() - reserve - ED` (`references/sails-syscall-mapping.md`: `value_available` -> `gcore::exec::value_available()`), so bounced or donated VARA can at least be recovered.
