## `Vault::fund_rewards` in programs/vault/app/src/lib.rs (L826-L842)

**Purpose:** Anyone pulls `assets` of the underlying from themselves into the vault's rewards tranche; the tokens vest linearly (comment L822-L824). Exported as `FundRewards(assets) -> Result<U256, VaultError>` (IDL L146), returning `holdings` after funding.

**Inputs & Assumptions:**
- `assets` (U256): payload. Trust: untrusted. Only `is_zero` is checked (L831); no minimum, no pause gate, no admin gate.
- Implicit: `message_source()` pre-await (L828) — **not** routed through `actor_for`, so a session key funding rewards pulls from its own balance, not the owner's; `now_ms()` post-await only (L837); gas via `ensure_gas`.
- Precondition: the sender approved the vault on the token (demo L209). In the tests the reward source is the token admin minting to itself (`Admin::mint`, demo L288-L298, gtest L162-L166); the vault is not a minter (gtest L115) and never mints.

**Outputs & Effects:**
- **Before the await (L827-L833): no state writes.** Reads `underlying`.
- **Await (L834):** `pull_underlying(underlying, from, assets)`.
- **After, `Ok`:** `fund` writes `vest_amount`, `vest_start_ms`, `vest_end_ms`, `holdings` (L837); reads `holdings`, `vest_end_ms` (L838); emits `RewardsFunded{from, assets, holdings, vesting_ends_at}` (L840); returns `holdings`.
- **After, `Err` from `pull_underlying`:** returns at L834, nothing written.
- **After, `Err` from `fund` (L837 `?`):** `ZeroAmount` is unreachable (L831 already rejected); `Overflow` at L427 leaves nothing written; `Overflow` at L438 leaves the vest fields written and `holdings` not — tokens are in the vault, counted as locked rewards but not as holdings, so `distributable` is *reduced* by their locked part.

**Block-by-Block:**

```rust
// L827-L833
ensure_gas(GAS_ONE_CALL)?;
let from = Syscall::message_source();
let underlying = {
    let s = self.state.borrow();
    if assets.is_zero() { return Err(VaultError::ZeroAmount); }
    s.underlying
};
```
- **What:** Gas, sender, zero gate, token id.
- **Why here:** Minimal pre-await validation; `paused` is deliberately or accidentally not consulted (no `ensure_live`, unlike `check_deposit` L379).
- **Establishes:** `assets > 0`, `from` fixed.
- **Depended on by:** L834, L837, L840.

```rust
// L834
pull_underlying(underlying, from, assets).await?;
```
- **What an interleaved message can observe between L834 and L835:** the vault unchanged by this call. What it can change that `fund` then meets: `vesting_period_ms` (`set_config`) — the new period is used at L426; the current tranche (`fund` from another funder) — folded in at L425; `total_shares` dropping to zero (a full exit) — `fund` still runs and the vested part is later swept to fees by the next deposit (L370-L375); `pause` — no effect.
- **Establishes (on `Ok`):** tokens in the vault.

```rust
// L835-L841
let (holdings, vesting_ends_at) = {
    let mut s = self.state.borrow_mut();
    s.fund(assets, now_ms())?;
    (s.holdings, s.vest_end_ms)
};
self.emit_event(VaultEvent::RewardsFunded { from, assets, holdings, vesting_ends_at }).expect("event");
Ok(holdings)
```
- **What:** Start/extend the tranche, count, report.
- **Assumes:** `emit_event` succeeds; a trap here discards `fund`'s writes while the tokens stay in the vault (uncounted).
- **Establishes:** rate unchanged at funding time, rising as the tranche vests.

**Cross-Function Dependencies:**
- Callees: `ensure_gas`, `pull_underlying`, `fund`.
- Callee (indirect, black box to the vault): token `Admin::mint` (demo L288-L298) is how rewards come into existence in the tests: `ensure_minter` (admin or minter set, L159-L161), then `mint` L120-L128 (`Paused`, `ZeroAddress`, `ZeroAmount`, supply `Overflow`, `credit` `Overflow`), then a VFT `Transfer{0 → to}` event and `Minted`. The vault neither calls nor depends on it; it accepts tokens from any approved holder.
- Callers: any actor (comment L823 "Anyone may fund rewards").
- Shared state: `vest_*` (only writer), `holdings`.
- Invariant couplings: since anyone can fund and funding is not paused, the rate can be moved upward by any token holder at any time (subject to vesting over at least one hour, L41). Dust funding cannot stretch a running tranche (test L1152-L1160).

**Open Questions:**
- unclear; need to inspect whether the absence of `ensure_live` on this path is intended: it is the one inbound flow that proceeds while the vault is paused.
- same `emit_event` and timeout questions.
