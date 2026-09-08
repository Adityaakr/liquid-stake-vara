## `push_underlying` in programs/vault/app/src/lib.rs (L725-L732)

**Purpose:** Pay `assets` of the underlying from the vault to `to` via the token's `transfer(to, assets)`, and translate the reply. It is the await inside `redeem` (L772), `claim` (L810) and `collect_fees` (L907); in every caller the payout has already been removed from `holdings` before this runs, and the caller compensates on `Err`.

**Inputs & Assumptions:**
- `underlying` (ActorId): as in `pull_underlying`.
- `to` (ActorId): the owner (`redeem`, `claim`) or the admin-supplied recipient (`collect_fees`, checked non-zero at L903). Trust: semi-trusted / admin-trusted.
- `assets` (U256): `p.net` (L772, `>= 1` by L403), `entry.assets` (L810, `>= 1` by L447 at request time), or the fee amount (L907, `>= 1` by L485). Trust: internal.
- Implicit: `TOKEN_CALL_GAS`, default 100-block `wait_up_to`, `message_source() = vault` on the token side.

**Outputs & Effects:**
- Sends `Transfer(to, assets)` (token_client.rs L159-L165, entry 7 L185) and awaits.
- Arms as in `pull_underlying`: `Ok(Ok(true))` → `Ok(())`; `Ok(Ok(false))` unreachable with the demo token (L201 only returns `Ok(true)`); `Ok(Err(e))` → `TokenRejected` (the token trapped: `Paused` L98, `ZeroAddress` L99, `ZeroAmount` L100, `InsufficientBalance` L101 when the vault's real token balance is below `assets`, `Overflow` L102 on the recipient); `Err(e)` → `TokenCall` (timeout, out-of-gas in the token, unavailable actor, undecodable reply).

**Block-by-Block:**

```rust
// L726
match DemoTokenProgram::client(underlying).vft().transfer(to, assets).with_gas_limit(TOKEN_CALL_GAS).await {
```
- **What:** The outbound transfer.
- **Why here:** After the caller's pre-await writes are persisted at this await; those writes are what make the interleaving safe (doc L16-L17).
- **Assumes:** the vault's token balance is at least `assets`. The vault never reads `balance_of(vault)`; it relies on `holdings` mirroring the balance. The reserve checks in the callers compare against `holdings`-derived numbers (L405, L464, L486), so the transfer fails with `InsufficientBalance` only if `holdings` overstates the balance — nothing found that can make it do so within this program (all `holdings` increases follow a confirmed inbound transfer, all decreases precede an outbound one and are restored on failure), except the timeout ambiguity below.
- **Establishes:** on `Ok(Ok(true))`, the token committed the payout.
- **Depended on by:** the `Ok` arms of the three callers (event + return).

```rust
// L727-L730
Ok(Ok(true)) => Ok(()),
Ok(Ok(false)) => Err(...TokenRejected...),
Ok(Err(e)) => Err(...TokenRejected...),
Err(e) => Err(...TokenCall...),
```
- **What:** Same mapping as `pull_underlying`.
- **Assumes:** every `Err` means "no tokens left the vault", which the callers rely on to run `revert_redeem` / `restore_unbond` / `restore_fees`. True for a trap and for gas/unavailable errors; for `Timeout` nothing found. If the token later executes the transfer, the vault has both paid and restored: `holdings` then overstates the balance by `assets` and, in `redeem`, the shares are back with the holder.
- **Depended on by:** `redeem` L772-L782, `claim` L810-L819, `collect_fees` L907-L916.

**Cross-Function Dependencies:**
- Callee: token `Vft::transfer` (external, source available: demo L197-L202) — `from = message_source()` is the vault; the only state it reads is `paused` and the two balances; on `Err` it traps (`unwrap_result`), so nothing partial persists. Its `emit_event(...).expect("event")` at L200 is a plain-string panic → `TokenCall` arm.
- Callee: token `Admin::pause` (demo L366-L375) is the admin-only switch that turns every later `push_underlying` into `TokenRejected(Paused)`; the vault then compensates each attempt and no payout can leave until the token resumes. The vault's own `paused` is separate (L196).
- Callers: `redeem`, `claim`, `collect_fees`.
- Shared state: none in the vault.
- Invariant couplings: `holdings == balance_of(vault) - uncounted stray transfers` holds iff every `Err` here truly means no transfer.

**Open Questions:**
- same timeout question as `vault-pull_underlying.md`.
