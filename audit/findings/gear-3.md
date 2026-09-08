# gear-3: Value attached to any non-payable command (and to the success path of every command) is kept on the program balance, invisible to `reserve`/`holdings`, and cannot be recovered

**Location**
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:681-696` and `:761-775` (`stake`, `fund_rewards`: the only two exports that read `Syscall::message_value()` and refund on `Err` with `CommandReply::with_value`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:700-942` (every other `Pool` export: `unstake`, `request_unbond`, `claim`, `set_config`, `pause`, `resume`, `transfer_admin`, `collect_fees`, `create_session`, `accept_session`, `revoke_session` and all queries) and `:560-619` (`Vft` exports)
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:734-1003`, `/Users/adityakrx/liquid-stake/programs/demo-token/app/src/lib.rs:186-460` (same for the vault and the token; they have no VARA accounting at all)

**Bug class** Execution model: attached value is delivered on every successful execution; only two handlers account for it.

**Severity** Low

**Description**

Gear delivers the attached value to the program whenever the execution *succeeds*, regardless of what the handler returned: `core-processor-1.10.0/src/processing.rs:576-596` (`process_success`: `SendValue { from: origin, to: program_id, value }`). A typed `Err` returned from a plain `#[export]` is a successful execution; Sails replies with `reply_bytes(encoded_result, value)` where `value` is the `CommandReply` value, `0` unless the handler used `with_value` (`sails-rs-1.0.1/src/gstd/mod.rs:37-46`, `src/gstd/macros.rs:283-296`). Only `stake` and `fund_rewards` build a `CommandReply` (`lib.rs:695`, `:774`); every other command, every query, and the success path of the two payable commands beyond the amount they book, leaves the value on the program.

The two exceptions that do refund are the `#[export(unwrap_result)]` `Vft` commands *on error only*: `ok_or_throws!` panics (`macros.rs:306-322`), the execution fails, and `process_error` returns the value with the system error reply (`processing.rs:297-311, 338-364`). On success they keep it.

The pool's `reserve` doc (`lib.rs:166-168`) states plain transfers are invisible to accounting, and there is no function that reads `Syscall::value_available()` or moves the difference between balance and `reserve`. In the vault and token there is no VARA accounting at all; the deploy script even tops each program up with 15 VARA "for outgoing messages" (`scripts/deploy.ts:40,107,139`), which the programs never spend (outbound gas comes from the caller's message, not the program balance).

**Trigger** A user or integrator attaches value to `unstake`, `claim`, `request_unbond`, a session call, a query, or a `Vft::transfer` (e.g. a wallet that always sets a value, or a program that forwards `msg::value()` by habit). The app's adapter never does this, so it is a footgun rather than an attack.

**Impact** The attached VARA is donated to the program and lost to everyone: it is not counted in `reserve`, not paid by `collect_fees`, and there is no sweep or `exit`. No third party is harmed; the pool's `balance >= reserve` invariant only becomes looser.

**Confidence** High.

**Suggested fix** In every non-payable command (and query) assert `Syscall::message_value() == 0` up front; a panic makes the runtime return the value with the error reply. Alternatively return `CommandReply::new(Err(..)).with_value(value)` from those handlers. Pair it with the admin `sweep_untracked()` from gear-2 so value that already landed can be recovered.
