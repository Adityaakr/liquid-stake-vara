## `Vault::deposit` in programs/vault/app/src/lib.rs (L738-L757)

**Purpose:** The inbound entrypoint: validate, pull the underlying from the owner, then mint shares at the post-transfer price. Exported as `Deposit(assets) -> Result<U256, VaultError>` (client IDL L145), so an `Err` is a normal reply carrying the error, not a trap; state written before an `Err` return stays written.

**Inputs & Assumptions:**
- `assets` (U256): payload. Trust: untrusted.
- Implicit: `Syscall::message_source()` read once, pre-await (L743); `now_ms()` read twice (L742 pre-await, L750 post-await); gas via `ensure_gas`.
- Precondition: the owner has approved the vault on the token for at least `assets`. Not checked by the vault; enforced by the token at demo L209 (`InsufficientAllowance` → `TokenRejected`).

**Outputs & Effects:**
- **Before the await (L739-L746): no state writes.** Reads: `session_keys`/`sessions` (via `actor_for`), `paused`, pricing state (`check_deposit`), `underlying`. The immutable borrow is dropped at L746 before the await (doc rule 1, L15).
- **Await (L747):** `pull_underlying` → one message to the token, state persisted, other messages run.
- **After the await, on `Ok`:** `settle_deposit` writes `fees_accrued` (sweep), `balances[owner]`, `total_shares`, `holdings` (L751); reads `rate_at(now)` (L752); emits VFT `Transfer{0 → owner, shares}` on route 1 (L754) and `Deposited{owner, assets, shares, rate}` (L755); returns `shares`.
- **After the await, on `Err` from `pull_underlying`:** returns the error at L747 with no state change in the vault.
- **After the await, on `Err` from `settle_deposit` (L751 `?`):** the tokens are in the vault, the sweep at L389 may have landed, no shares minted, `holdings` unchanged, error returned. Reachable only on `Overflow` paths (L390-L392).

**Block-by-Block:**

```rust
// L739-L746
ensure_gas(GAS_ONE_CALL)?;
let (owner, underlying) = {
    let s = self.state.borrow();
    let now = now_ms();
    let owner = s.actor_for(Syscall::message_source(), SessionAction::Deposit, now)?;
    s.check_deposit(assets, now)?;
    (owner, s.underlying)
};
```
- **What:** Gas gate, resolve who the message acts for, validate, capture `underlying`.
- **Why here:** All rejections happen before any message leaves; `owner` is fixed now so a session revoked during the await does not change who gets the shares.
- **Assumes:** `check_deposit`'s answer stays valid across the await — it does not (see below); the design accepts that and re-prices.
- **Establishes:** `owner` (sender or session owner), `assets >= MIN_DEPOSIT`, not paused *now*.
- **Depended on by:** L747 (`from = owner`), L751.

```rust
// L747
pull_underlying(underlying, owner, assets).await?;
```
- **What:** The transfer-in. Pulls from `owner`, so a session key spends the owner's balance and allowance (gtest L336-L341).
- **Why here:** After validation, before any write.
- **What an interleaved message can observe between L747 and L749:** the vault exactly as before this deposit (no pre-await writes). What it can change: `paused` (`pause` L859); `instant_fee_bps`/periods (`set_config` L847); `holdings`/`vest_*` (`fund_rewards`); `total_shares`/`holdings`/`fees_accrued` (other `deposit`s, `redeem`s, `request_unbond`s, `claim`s, `collect_fees`); `base_rate` (a `burn_shares` that empties the vault); `balances[owner]` (share transfers); the owner's session (`revoke_session`, `accept_session` — irrelevant, `owner` is captured); and, on the token side, anything — the owner's remaining balance/allowance, the token's `paused`. None of these is re-validated except through `shares_for` at L390.
- **Establishes (on `Ok`):** `assets` more tokens in the vault than `holdings` counts.
- **Depended on by:** L751.

```rust
// L748-L753
let (shares, rate) = {
    let mut s = self.state.borrow_mut();
    let now = now_ms();
    let shares = s.settle_deposit(owner, assets, now)?;
    (shares, s.rate_at(now))
};
```
- **What:** Sweep, re-price, mint, count; then the post-deposit rate for the event.
- **Why here:** Post-await, fresh borrow, fresh timestamp (L750).
- **Assumes:** `settle_deposit` does not need `paused == false` — it has no `ensure_live`, so a pause landing in the window does not block settlement.
- **Establishes:** shares minted at the settlement-time price, `>= 1` (L390 floor; the pre-await `shares == 0` rejection at L383 is not re-applied).
- **Depended on by:** L754-L756.

```rust
// L754-L756
emit_vft_transfer(self.state, ActorId::zero(), owner, shares);
self.emit_event(VaultEvent::Deposited { owner, assets, shares, rate }).expect("event");
Ok(shares)
```
- **What:** Two events, then the reply.
- **Why here:** After all writes.
- **Assumes:** `emit_event` succeeds; both use `.expect("event")` (L667, L755). A panic here is a trap in the *post-await* execution: Gear discards this execution's writes — the mint and `holdings` update at L751 — while the token transfer at L747 stands. Conditions under which `emit_event` returns `Err` are not in this file (open question).
- **Depended on by:** indexers (event) and the client (reply).

**Cross-Function Dependencies:**
- Callees `ensure_gas`, `actor_for`, `check_deposit`, `pull_underlying`, `settle_deposit`, `rate_at`, `emit_vft_transfer` (L664-L668, re-exposes the `Vft` service at route 1 to emit on its behalf): records in the sibling files.
- Callers: any actor via the `Vault` service; a session key mapped to an owner with `Deposit` in its actions.
- Shared state: all pricing state; the token's balances/allowances.
- Invariant couplings: the accounting bound is preserved because nothing is written pre-await and `settle_deposit` only adds to `holdings`. `holdings == token balance` holds on the `Ok(Ok(true))` path; the `Timeout` path in `pull_underlying` and the `settle_deposit` `Err` path both leave tokens in the vault uncounted.

**Open Questions:**
- unclear; need to inspect `sails_rs::gstd::services::Service::emit_event` for its failure conditions (gas for the event message, payload size) to know when the post-await `.expect` can trap.
- unclear; need to inspect whether a paused vault is meant to finish in-flight deposits (see `vault-settle_deposit.md`).
