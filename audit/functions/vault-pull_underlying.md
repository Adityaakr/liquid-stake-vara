## `pull_underlying` in programs/vault/app/src/lib.rs (L714-L722)

**Purpose:** Move `assets` of the underlying from `from` into the vault by asking the token program to execute `transfer_from(from, vault, assets)`, and translate the reply into the vault's error vocabulary. It is the await inside `deposit` (L747) and `fund_rewards` (L834); the vault counts the assets only when this returns `Ok(())`.

**Inputs & Assumptions:**
- `underlying` (ActorId): the token program id stored at construction (L1019, L173). Trust: trusted (deployer-chosen); no check that it is a program or that it speaks this interface. The client is built with `DemoTokenProgram::client(underlying)` (sails-rs-1.0.1 `src/client/mod.rs` L67-L69), a plain `Actor::new(GstdEnv, program_id)`.
- `from` (ActorId): in `deposit`, the *owner* resolved by `actor_for` (L743), not necessarily the message sender; in `fund_rewards`, the message sender (L828). Trust: semi-trusted.
- `assets` (U256): payload amount. Trust: untrusted.
- Implicit: `Syscall::program_id()` (L715) as `to`; `TOKEN_CALL_GAS = 6e9` (L46) as the callee's gas; default `wait_up_to` of 100 blocks (gstd `config.rs` L73; `msg/macros.rs` L47-L55 applies it when `params.wait_up_to` is `None`, which `with_gas_limit` leaves untouched).

**Outputs & Effects:**
- Sends one message to `underlying` with payload `TransferFrom(from, me, assets)` (token_client.rs L166-L173, encoded with the `Vft` interface id and route 1, L120-L123, L186) and awaits the reply. Gear persists the vault's state at the await.
- Returns `Ok(())` only on `Ok(Ok(true))` (L717).
- `Ok(Ok(false))` → `Err(TokenRejected("transfer returned false"))` (L718). The demo token never returns `Ok(false)` (its `transfer_from` returns only `Ok(true)` at L213 or traps), so this arm is reachable only with a different underlying.
- `Ok(Err(TokenError))` → `Err(TokenRejected(debug string))` (L719). Reached when the token's `#[export(unwrap_result)]` method returned `Err`: the export macro wraps the call in `ok_or_throws!` (sails-macros-core-1.0.1 `src/service/exposure.rs` L97-L103), which encodes the error with the method's sails header and calls `Syscall::panic(&encoded)` (sails-rs-1.0.1 `src/gstd/macros.rs` L307-L325). The token execution therefore **traps** — its state changes for that message are discarded — and the vault's client receives an error reply with `ErrorReplyReason::Execution(UserspacePanic)`, whose payload `decode_reply_or_throw` (`src/client/mod.rs` L450-L465) decodes as `TokenError` via `decode_error` (L675-L680, header-checked by `decode_with_header` L402-L425). gtest L143-L152 exercises `InsufficientAllowance` this way.
- `Err(sails Error)` → `Err(TokenCall(debug string))` (L720). Reached on: `Error::Timeout` after 100 blocks without a reply (gstd `msg/async.rs` L42-L46); any error reply that is not a decodable userspace panic — the token ran out of its 6e9 gas, the token's `.expect("event")` at demo L212 panicked with a plain string (header decode fails, falls through to `Err(err)` at `mod.rs` L462), the destination is not a program / exited (`UnavailableActor`), `UnsupportedReply`; or a decode failure of a success reply (wrong interface/route/entry id, `mod.rs` L415-L423), which is the arm a non-conforming `underlying` lands in.

**Block-by-Block:**

```rust
// L715-L716
let me = Syscall::program_id();
match DemoTokenProgram::client(underlying).vft().transfer_from(from, me, assets).with_gas_limit(TOKEN_CALL_GAS).await {
```
- **What:** Build the call, cap the callee's gas, send, wait.
- **Why here:** The only external interaction of the inbound flows; everything before it in the caller is read-only (L740-L746, L829-L833).
- **Assumes:** the token honours `spender = message_source() = vault` semantics: demo `transfer_from` L206-L210 spends `allowance(from, vault)` unless `from == vault`, then moves `value` from `from` to `me`. So the owner must have approved the vault (gtest L93); a session key depositing pulls from the owner's balance and allowance, not the key's (gtest L336-L341).
- **Establishes:** on `Ok(Ok(true))`, that the token committed a transfer of exactly `assets` from `from` to the vault — for the demo token, because its handler only replies `Ok(true)` after `s.transfer` returned `Ok` (L210, L213) and a successful execution commits.
- **Depended on by:** `settle_deposit` L751 and `fund` L837, which add `assets` to `holdings` on the strength of this.

```rust
// L717-L720
Ok(Ok(true)) => Ok(()),
Ok(Ok(false)) => Err(VaultError::TokenRejected("transfer returned false".into())),
Ok(Err(e)) => Err(VaultError::TokenRejected(format!("{e:?}"))),
Err(e) => Err(VaultError::TokenCall(format!("{e:?}"))),
```
- **What:** Four arms to two error kinds.
- **Why here:** The caller only distinguishes `Ok` from `Err` (`?` at L747, L834); the string is for the client.
- **Assumes:** every `Err` means "no tokens moved". True for `Ok(Err)` (trap) and for gas/unavailable error replies; for `Timeout` nothing found — the token message may still be in the queue and execute later, after the vault has already replied `Err` to the depositor. In that case the tokens land in the vault uncounted by `holdings` (they become invisible, comment L186-L187), and the depositor holds no shares.
- **Establishes:** the caller's early return leaves vault state untouched (no pre-await writes in either caller).
- **Depended on by:** `deposit` L747, `fund_rewards` L834.

**Cross-Function Dependencies:**
- Callee: token `Vft::transfer_from` (external, source available: demo-token `app/src/lib.rs` L205-L214). Paths: `spender != from` → `spend_allowance` L112-L118 (`U256::MAX` allowance is never decremented L114; otherwise `checked_sub` → `InsufficientAllowance`); then `transfer` L97-L103: `Paused`, `ZeroAddress` (`me` is never zero), `ZeroAmount` (the vault rejects zero before calling, L380/L831), `InsufficientBalance` (`debit` L92), `Overflow` (`credit` L85); then the event L212 (`expect`, plain panic); then `Ok(true)`. Every `Err` traps, so partial effects (allowance decremented, balance not) never persist.
- Callee: the sails client machinery (external, source available): `PendingCall::poll` (`gstd_env.rs` L246-L291) sends on first poll and decodes on wake; `redirect_on_exit` is off by default so a `ProgramExited` reply is returned as `Err` (L281-L287).
- Callers: `deposit` L747, `fund_rewards` L834.
- Shared state: none in the vault; token-side `balances`/`allowances`.
- Invariant couplings: `holdings` counts exactly the sum of amounts for which this returned `Ok` (plus `fund`), minus outbound amounts; any divergence between that and the token's `balance_of(vault)` is invisible to the vault.

**Open Questions:**
- unclear; need to inspect Gear 1.10 runtime semantics for a message whose reply arrives after the waiter's `wait_up_to` expired — whether the sending program's continuation already ran with `Timeout`, and whether the late reply is dropped (gstd `signals.rs` L68-L80 records it against a completed future).
- unclear; need to inspect whether any deployed `underlying` is not the demo token; the `Ok(Ok(false))` and header-mismatch arms only matter then.
