## `Vault::redeem` in programs/vault/app/src/lib.rs (L761-L783)

**Purpose:** Instant exit: burn shares and pay `net` at the current rate minus the instant fee. All accounting happens before the await; the transfer happens after; failure is compensated. Exported as `Redeem(shares) -> Result<RedeemPreview, VaultError>` (IDL L160).

**Inputs & Assumptions:**
- `shares` (U256): payload. Trust: untrusted.
- Implicit: `message_source()` pre-await (L766), `now_ms()` once pre-await (L765) — the post-await segment does not read the clock; gas via `ensure_gas`.

**Outputs & Effects:**
- **Before the await (L762-L771):** `begin_redeem` writes `balances[owner]`, `total_shares`, maybe `base_rate`, `fees_accrued += fee`, `holdings -= net` (L768); `rate` is read *before* `begin_redeem` (L767) so the `Redeemed` event carries the pre-exit rate. The `Transfer{owner → 0, shares}` VFT event is emitted pre-await (L771). The mutable borrow ends at L770.
- **Await (L772):** `push_underlying(underlying, owner, preview.net)`.
- **After, `Ok`:** emits `Redeemed{owner, shares, assets, fee, rate}` (L774), returns the preview. No further state writes.
- **After, `Err`:** `revert_redeem` (L778) re-mints shares, `fees_accrued -= fee` saturating, `holdings += net`; emits `Transfer{0 → owner, shares}` (L779); returns the error. The `Redeemed` event is not emitted; an indexer sees burn then mint.

**Block-by-Block:**

```rust
// L762-L770
ensure_gas(GAS_ONE_CALL)?;
let (owner, underlying, preview, rate) = {
    let mut s = self.state.borrow_mut();
    let now = now_ms();
    let owner = s.actor_for(Syscall::message_source(), SessionAction::Redeem, now)?;
    let rate = s.rate_at(now);
    let p = s.begin_redeem(owner, shares, now)?;
    (owner, s.underlying, p, rate)
};
```
- **What:** Gas, actor, rate snapshot, the whole accounting step.
- **Why here:** Everything the payout depends on is decided and persisted before the message leaves (doc L16-L17).
- **Establishes:** `preview.net >= 1`, `preview.assets <= distributable` at `now`; shares gone; fee booked; `holdings` reduced by `net`.
- **Depended on by:** L771-L782.

```rust
// L771-L772
emit_vft_transfer(self.state, owner, ActorId::zero(), shares);
match push_underlying(underlying, owner, preview.net).await {
```
- **What:** Burn event, then the payout.
- **What an interleaved message can observe between L772 and L773:** the holder's shares already gone, `total_shares` reduced, `fees_accrued` including this fee, `holdings` excluding `net`; hence `distributable` and `rate_at` as if the exit had completed (test L1114). What it can change that the post-await segment then meets: `collect_fees` can pay out `fees_accrued` including this `fee` (so `revert_redeem`'s `saturating_sub` floors, see `vault-revert_redeem.md`); `deposit`/`fund_rewards`/other exits move `holdings` and `total_shares` (the revert is additive, so it composes); a `burn_shares` elsewhere that empties the vault writes `base_rate` from the reduced state; `pause` has no effect on the revert (no gate); `revoke_session` has no effect (owner captured); a share transfer *to* `owner` is fine (revert adds). On the token side, `Admin::pause` (demo L366) makes the transfer trap → `TokenRejected(Paused)` → revert.
- **Establishes (on `Ok`):** the token paid `net` to `owner`.
- **Depended on by:** L773-L781.

```rust
// L773-L781
Ok(()) => { self.emit_event(VaultEvent::Redeemed { ... }).expect("event"); Ok(preview) }
Err(e) => {
    self.state.borrow_mut().revert_redeem(owner, shares, &preview);
    emit_vft_transfer(self.state, ActorId::zero(), owner, shares);
    Err(e)
}
```
- **What:** Confirm or compensate.
- **Why here:** Post-await execution; the compensation is a plain function that cannot fail (L413-L417).
- **Assumes:** `Err` from `push_underlying` means the token did not pay (see `vault-push_underlying.md`; `Timeout` is the arm nothing establishes). Assumes `emit_event`/`emit_vft_transfer` do not trap: a trap at L774 discards nothing of consequence (no writes in this segment besides the event); a trap at L779 discards the `revert_redeem` writes at L778 — the shares stay burnt and `holdings` stays reduced while no tokens left the vault (the assets become orphaned dust in `distributable`, benefiting remaining holders).
- **Establishes:** on `Ok`, state and token balances agree; on `Err`, state as before modulo interleavings.
- **Depended on by:** the client's reply; indexers.

**Cross-Function Dependencies:**
- Callees: `ensure_gas`, `actor_for`, `rate_at`, `begin_redeem`, `emit_vft_transfer`, `push_underlying`, `revert_redeem`.
- Callers: any actor; a session key with `Redeem`.
- Shared state: `fees_accrued` with `collect_fees` (the interleaving above); all pricing state.
- Invariant couplings: the accounting bound is preserved on every path (see `vault-distributable.md`). `sum(balances) == total_shares` preserved except on the discarded `mint_shares` overflow in `revert_redeem` L414.

**Open Questions:**
- same `emit_event` failure-conditions question as `vault-Vault-deposit.md`; here a trap at L779 is the one that leaves compensation half-done.
- same timeout question.
