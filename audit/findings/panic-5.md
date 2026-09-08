# panic-5: Any trap in the post-await segment of `deposit` / `fund_rewards` strands the already-pulled tokens; the only panic sites there are the two `expect("event")` calls

- **Location**: `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:747-756` (`Vault::deposit`: `pull_underlying(...).await?` at `:747`, then `settle_deposit` `:751`, `emit_vft_transfer` `:754` and `.expect("event")` `:755`); `:826-843` (`Vault::fund_rewards`, same shape; `expect("event")` at `:842`); the `expect("event")` inside `emit_vft_transfer` at `:667`
- **Bug class**: UNWRAP/ASSERTREACH after an `.await` (panic-after-await, Gear persists state at every await)
- **Severity**: Low (consequence is fund loss; trigger is not attacker-controllable today)
- **Confidence**: High on the consequence; Low on reachability

## Description

`deposit` pulls the underlying with `transfer_from` in the first execution segment, then
`.await`s. The token moves the funds and replies; the vault wakes in a *new* execution
that does `settle_deposit` (mint shares, `holdings += assets`) and emits two events with
`.expect("event")`. If that wake execution traps for any reason, Gear reverts only that
execution's state (so no shares, no `holdings` bump) and replies with an error, while the
tokens have already left the user and sit in the vault's token balance. Because
`holdings` is internal accounting ("a plain token transfer to the vault is invisible
here", line 175), those tokens are not part of `distributable`, cannot be swept by
`sweep_orphans`, and there is no rescue function: they are lost.

Panic sites in the post-await segment, audited one by one:

- `self.state.borrow_mut()` at `:749`: no other borrow is live (the pre-await borrow scope
  closed at `:746`), and `emit_vft_transfer` does not borrow the cell (`Vft::new(state).expose(..)` only wraps the reference; `emit_event` is `msg::send_bytes(ActorId::zero(), ..)` in sails-rs 1.0.1 `src/gstd/events.rs:92-101`). No REFCELLPANIC.
- `settle_deposit`: `sweep_orphans` saturating; `shares_for` divides by `base_rate` (>= SCALE) or by `distributable` guarded by `empty()`; `mint_shares`/`holdings` use `checked_*` and return `Err` (not a trap). `rate_at` divides by `total_shares` guarded by `empty()`. No ARITHOFL.
- `.expect("event")` x2: `emit_event` fails only if `msg::send_bytes` fails (outgoing message limit, memory). Not reachable with three outgoing messages per handler, but it is the one trap the code itself introduces after the await.
- Out-of-gas in the wake segment is the realistic trap: the segment runs on whatever gas
  the original message had left after `TOKEN_CALL_GAS` and the first segment
  (`GAS_ONE_CALL = 40B` is the entry guard). Gas budgeting is outside this cluster; flagging
  that the same stranded-tokens consequence applies.

Note also that `settle_deposit` returning `Err` (only `Overflow`, unreachable in practice)
has the same effect via `?`: the tokens are pulled, no shares are minted, and here the
error reply does not even revert `sweep_orphans`.

## Concrete trigger

None attacker-reachable found. Would need `msg::send_bytes` to fail in the wake segment or
the wake segment to run out of gas.

## Impact

If it ever fires: depositor's tokens are stranded in the vault forever (no `holdings`
credit, no rescue path). Same for `fund_rewards`.

## Suggested fix

1. Never trap after the await: replace `.expect("event")` with a best-effort emit
   (`let _ = ...` or a logged error) in `deposit`, `fund_rewards`, and `emit_vft_transfer`
   when called from an async handler; or emit both events before the await is impossible
   here, so keep them best-effort.
2. Add an admin `sync_holdings` / rescue entry point that reconciles `holdings` with the
   vault's actual token balance (query `balance_of(vault)` on the underlying) and books the
   surplus into `fees_accrued`, so a stranded deposit can be recovered off-chain.
3. Keep the gas guard conservative and cover the wake segment in gtest with a low-gas
   deposit to confirm the reserved budget is sufficient.
