# Gear execution-model cluster: notes, non-findings and unverified points

Scope: `programs/vara-pool/app/src/lib.rs` (P), `programs/vault/app/src/lib.rs` (V), `programs/demo-token/app/src/lib.rs` (T). Sources read: the vara-skills 3.0.0 references (`gear-execution-model.md`, `gear-messaging-and-replies.md`, `gear-gas-reservations-and-waitlist.md`, `sails-syscall-mapping.md`, `gear-sails-production-patterns.md`, `gear-gstd-api-and-syscalls.md`) plus the vendored `core-processor-1.10.0`, `gear-core-1.10.0`, `gear-core-errors-1.10.0`, `gear-common-1.10.0`, `gstd-1.10.0`, `gstd-codegen-1.10.0`, `gtest-1.10.0`, `sails-rs-1.0.1`, `sails-macros-core-1.0.1` under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. pallet-gear / pallet-gear-bank / the Vara runtime are not on disk; anything that lives only there is marked UNVERIFIED and gtest's mirror is cited instead.

## Q1: `send_bytes_with_gas(to, [], 0, value)`

- Accepted for any destination: `core-processor-1.10.0/src/ext.rs:461-479` allows gas 0 or gas >= mailbox threshold; `0 < gas < threshold` fails with `InsufficientGasLimit`.
- User destination: explicit `Some(0)` -> not the mailbox path (`gtest-1.10.0/src/manager/send_dispatch.rs:282-320`, the `gas_limit >= threshold` branch cuts/locks gas and stores the message; 0 never reaches it), value transferred immediately through the bank. The comment at P:670-671 is correct. Had the code used `send_bytes` (gas `None`), gtest takes the threshold from the parent node when available (`send_dispatch.rs:284-297`) and the payout would sit in the mailbox until claimed or expired, so the explicit 0 is load-bearing; keep it.
- Vara mainnet `MailboxThreshold` value: UNVERIFIED here (irrelevant while the gas is 0).
- Program destination: message queued with a 0-gas node, precharge fails, value bounces to the pool balance with a system error reply. Finding gear-2.
- Delivery to an account below ED: dropped, not refused. Finding gear-4.
- `ActorId::SYSTEM` as `to` (only reachable via `collect_fees(to)`): `check_forbidden_destination` (`ext.rs:727-733`) makes the send fail -> `pay_out` returns `Err` -> fees restored. Fine.
- `pay_out`'s `Ok(())` means "queued", never "delivered". Every payout path treats it as delivered.

## Q2: value on `Err` without `with_value`

- Only `stake` and `fund_rewards` refund (P:695, P:774). All other commands, all queries, and all three programs keep attached value on success or typed `Err` (`process_success` `SendValue` to the program, `core-processor-1.10.0/src/processing.rs:576-596`). `#[export(unwrap_result)]` Vft commands refund on error via the trap path (`processing.rs:297-364`), keep on success. Finding gear-3.
- `stake` refund path: `reply_bytes(encoded, value)` -> `reply_commit` -> `charge_message_value(value)` against a counter initialised as `free balance + message value` (`core-processor-1.10.0/src/executor.rs:96-101`), so the refund can never fail on value; the program's balance ends where it started. No ED check exists on reply value in 1.10 (see gear-4), and users cannot attach `0 < value < ED` from an extrinsic anyway (pallet-gear `ValueLessThanMinimal`, UNVERIFIED: pallet not vendored).
- The vault has no VARA accounting; the 15 VARA the deploy script sends each program (`scripts/deploy.ts:40,107,139`) is never spent by the programs (outbound gas comes from the caller's message: `ext.rs:452-456` reduces the *message's* gas counter), so it is only an ED/rent cushion.

## Q3: vault async flows

- Token domain errors arrive as error replies (the token uses `unwrap_result` -> `ok_or_throws!` panic payload) and the Sails client decodes them into `Ok(Err(TokenError))`; the vault test at `programs/vault/tests/gtest.rs:149` confirms `TokenRejected(.. InsufficientAllowance ..)`. Both `Ok(Err(_))` and `Err(_)` arms compensate, which is right for real error replies and wrong for `Timeout`. Finding gear-1.
- Gas ownership, stated precisely: the *reply-handling entrypoint* (`handle_reply` -> `record_reply` -> `gr_wake`) is paid by the reply's gas node, which without a reply deposit is an `UnspecifiedLocal` split of the token message's 6B (`gtest journal.rs:173-176`, `gear-common node.rs:213-218`). The *continuation* after the wake is paid by the original message's node. `ensure_gas(GAS_ONE_CALL)` (V:709-711) covers only the continuation. `Syscall::gas_available()` is read inside the future, i.e. after `message_loop` has already taken `system_reserve_gas(10B)` (`gstd-1.10.0/src/async_runtime/futures.rs:70-77`, `config.rs:38`), so the 40B floor is net of the signal reserve. Budget after the check: 40B - 6B token cut - waitlist reserve (`ext.rs:1315-1335`, small) >= ~33B for a post-await segment documented at ~17B. Adequate today with ~2x margin; it is a constant, not a measurement, see gear-5.
- Waitlist rent while waiting is charged from the original message's gas per block (`ext.rs:1264-1283` reserve, then per-block hold cost); the Vara per-block waitlist cost is UNVERIFIED but is orders of magnitude below the budget for 100 blocks.
- Caller under-funding: cannot get past `ensure_gas` with less than 40B net of the 10B reserve; with more, the only way to persist state mid-flow without compensation is a trap in the post-await segment (gear-5). For `redeem`/`claim`/`collect_fees` that trap leaves a *consistent* state (position burned, tokens sent; only the event and the reply are missing); for `deposit`/`fund_rewards` it strands tokens.
- Token `RanOutOfGas` inside `TOKEN_CALL_GAS`: the system error reply has `gas_limit: None` (`gear-core-1.10.0/src/message/reply.rs:211-218`) on a node with ~0 gas, so the vault cannot process it; the vault message waits the full 100 blocks (`gstd config.rs:73`) and then compensates (correctly, because the token rolled back). UX cost: ~5 min lock of the user's message and a `TokenCall("Timeout")` reply instead of `RanOutOfGas`.
- Late reply after timeout: `record_reply` still finds the signal (the timeout branch in `gstd msg/async.rs:41-46` removes the lock but not the signal), sets its payload and calls `exec::wake` on a message that is no longer waiting; the runtime logs "Failed to wake unknown message" (`gtest journal.rs:206-247`). No panic, no state change; the `SIGNALS` entry leaks (bounded by one per timed-out call).
- `handle_signal`: Sails generates it and gstd's default only cleans futures/locks/hooks (`gstd async_runtime/mod.rs:80-93`); no vault hook. Fine, but nothing records which message failed (gear-5 suggestion).

## Q4: interleavings between the await and the continuation

No confirmed bug. Checked:
- `paused` flips between segments: `settle_deposit` (V:388-394) and `fund` (V:423-440) do not re-check `ensure_live`, so an in-flight deposit completes while paused. Acceptable (tokens already pulled; refusing would strand them), but worth documenting.
- `admin` / `underlying`: `underlying` has no setter; `collect_fees` fixes `to` and `amount` pre-await; `transfer_admin` mid-flight changes nothing the continuation reads.
- Sessions: `actor_for` resolves the owner pre-await (V:743, 766, 806); revoking or replacing the key mid-flight cannot redirect the payout.
- Same-user concurrency: `begin_redeem`/`take_unbond`/`take_fees` mutate pre-await, so a second `redeem` sees the reduced balance, a second `claim(id)` finds no entry, a second `collect_fees` gets `ZeroAmount`. Two in-flight deposits are independent `transfer_from`s.
- Rate drift: `deposit` mints at the *post-await* rate (V:751), which can differ from the pre-await preview (vesting drip, fees, other flows). No slippage parameter (`min_shares`), but no attacker lever was found: `fund_rewards` starts a tranche at `vest_start = now` (no immediate rate change), `sweep_orphans` only fires at `total_shares == 0`, and redeems are rate-neutral. Consider `deposit(assets, min_shares)` for integrators.
- `revert_redeem` after another deposit during the wait: shares and `holdings` are restored proportionally, `fees_accrued` reduced by the fee, so the rate is unchanged for the interloper. If the vault emptied pre-await (`burn_shares` pinned `base_rate`, V:362) and refilled during the wait, the revert simply re-adds the shares.
- `check_deposit` requires `shares > 0` pre-await but `settle_deposit` uses `.max(1)` post-await: at worst the depositor gets 1 share worth <= `assets`; vault-favourable rounding, not exploitable.
- The `redeem` burn event is emitted before the await (V:771); the compensation emits a matching mint (V:779). Events are materialised when the segment ends in `wait` (successful execution), matching `references/gear-execution-model.md`.

## Q5: `CommandReply::with_value` refunds and ED

- Value counter = `free balance + message value` (`executor.rs:96-101`); no ED subtraction; `deposit_value(from, value, keep_alive=false)` in the bank (`gtest journal.rs:159-161`). The refund of the full incoming value therefore always passes and leaves the balance unchanged.
- The pool's balance is `>= reserve + 15 VARA + (ED, see below)`, payouts are `<= reserve` (checked in `unstake`/`take_unbond`/`collect_fees`), so no payout can drive the program under ED. gtest asserts a freshly created program holds exactly ED (`gtest-1.10.0/src/system.rs:733`) and `create_program` charges ED from the creator (`ext.rs:1368-1373`); whether `upload_program` on-chain seeds ED the same way is UNVERIFIED (pallet not vendored).
- Bounced value (gear-2) and donated value (gear-3) only push the balance *above* `reserve`, never below.

## Q6: `message_source()` and programs as actors

- `Syscall::message_source()` -> `gcore::msg::source()` (`references/sails-syscall-mapping.md`); for a program-sent message it is the sending program's id. A program can therefore stake (value attached the same way), transfer kVARA, request/claim unbonds, propose a session (as owner) and accept one (as key). None of the guards distinguish program from user ids; nothing needs to, except `pay_out` (gear-2).
- A program acting as a session *key* is harmless (payouts go to the owner). A program acting as *owner* behind a session key hits gear-2 on every payout.
- Faucet, token admin and vault deposits by programs are fine: VFT balances are program state, not value.

## Other notes

- `TransferFailed` (P:59-60, 673) can only be raised for syscall-time failures: `NotEnoughValue` from the value counter (unreachable while `reserve <= balance`), forbidden destination, or a gas-limit rule violation (impossible with 0). It never reflects delivery.
- The pool comment at P:15-16 ("no partial states to compensate") is true for the pool's *own* execution but not for value delivery (gear-2, gear-4).
- README "Gas ... deposit or redeem costs about 0.8 VARA" and `src/chain/config.ts` `vaultAsync = 100B`: the frontend limit is comfortably above `GAS_ONE_CALL` (40B net of the 10B system reserve), so the app cannot trip `NotEnoughGas`; `pool = 40B` and `vaultSync = 40B` are generous for synchronous commands. `GAS.token = 30B` is for the token's own commands and is unrelated to the vault's internal 6B cut.
- `programs/vault/tests/gtest.rs` has no test for `NotEnoughGas`, for a token reply timeout, or for a program-address recipient in the pool tests; the gtest harness (`BlockRunMode`, `run_to_block`) can express all three (`references/gear-sails-production-patterns.md` §14).
- `sails-rs 2.0.0` / `gstd 2.0.0` are also in the registry; this review is pinned to the 1.10 / 1.0.1 sources the programs build against (`programs/*/Cargo.toml`, README "Programs").
