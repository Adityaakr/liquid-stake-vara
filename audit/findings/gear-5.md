# gear-5: A trap in the post-await segment of `deposit` / `fund_rewards` leaves the pulled tokens uncounted with no recovery path

**Location**
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:747-756` (`deposit`: `pull_underlying(..).await?` then `settle_deposit`, `emit_vft_transfer`, `emit_event(..).expect("event")`)
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:834-841` (`fund_rewards`: same shape)
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:388-394` (`settle_deposit` can return `Overflow`), `:14-19` (module doc: "Gear persists state at every `.await`")
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:708-711`, `:49-50` (`ensure_gas(GAS_ONE_CALL = 40B)` as the only protection)

**Bug class** Async execution model: rollback boundary is the execution segment, not the command; irreversible external effect (token pull) precedes the local commit with no reconciliation.

**Severity** Low

**Description**

Per `references/gear-execution-model.md` ("A panic or unrecoverable failure rolls back the current message execution") and the wait semantics in `core-processor-1.10.0/src/ext.rs:1264-1283` (a `wait` ends the execution successfully; state and outgoing messages are committed at that point), the segment after `.await` is its own execution. If it traps, only that segment's changes are discarded: the token has already executed `transfer_from`, the tokens sit in the vault's token balance, and `holdings`/shares are never updated. The user's message ends with an error reply; nothing schedules a retry or a refund, and there is no admin path that can move tokens that are not in `holdings` (`collect_fees` is bounded by `fees_accrued`; `push_underlying` is only reachable through redeem/claim/collect_fees).

What can trap there: `RanOutOfGas` if the remaining gas is below what the segment needs; `emit_event(..).expect("event")` / `emit_vft_transfer(..).expect("event")` if an event send fails (out of gas is the realistic cause); `settle_deposit` `Overflow` (unreachable at real magnitudes, see arith-notes). The gas budget is: caller's limit minus the first segment, minus gstd's `system_reserve_gas(10B)` taken in `message_loop` before the future is first polled (`gstd-1.10.0/src/async_runtime/futures.rs:70-77`, `config.rs:38`), then `ensure_gas` requires >= 40B, then 6B is cut for the token call and the waitlist reserve is charged (`ext.rs:1315-1335`); the post-await segment is documented at ~17B (`lib.rs:47-50`). Roughly 2x margin today, but `GAS_ONE_CALL` is a constant while the segment cost moves with runtime weights and with the vault's own state size (the `balances` map is loaded page by page). `references/gear-sails-production-patterns.md` §13 warns against fixed gas amounts for exactly this reason. Note also that the signal fired on the trap (system reserve) only runs gstd's cleanup (`gstd-1.10.0/src/async_runtime/mod.rs:80-93`); the vault registers no `handle_signal`.

**Trigger** A `deposit`/`fund_rewards` whose post-await segment runs out of gas (weight increase, larger state, or a caller that sends exactly the minimum and a first segment that is unusually expensive). Not attacker-driven; the caller only hurts themself.

**Impact** The depositor's tokens are stranded in the vault (not credited, not withdrawable, not sweepable). The vault's rate is unaffected because `holdings` was never increased.

**Confidence** High on the mechanism; Low on likelihood at the current constants (no measurement of the post-await segment against a 1.10 node in this review; the README's 17B figure is taken at face value).

**Suggested fix** Same reconciliation primitive as gear-1: keep a `pending_deposits: BTreeMap<MessageId, (owner, assets)>` entry written *before* the await and removed in `settle_deposit`; add an admin/keeper `settle_pending(id)` that mints the shares for entries whose tokens arrived (verified with a `balance_of(vault)` query or the token's Transfer event off-chain). Also add a `handle_signal` that records the failed message id so the operator can find stranded deposits. Consider deriving the required gas from a measured per-segment figure plus margin rather than a single constant.
