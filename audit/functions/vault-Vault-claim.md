## `Vault::claim` in programs/vault/app/src/lib.rs (L801-L820)

**Purpose:** Pay out a matured unbond entry. Entry removed and assets taken out of `holdings` before the await; restored on failure. Exported as `Claim(id) -> Result<U256, VaultError>` (IDL L142).

**Inputs & Assumptions:**
- `id` (u64): payload. Trust: untrusted; must be an entry under the resolved owner (L461-L462).
- Implicit: `message_source()` and `now_ms()` pre-await (L805-L806); gas via `ensure_gas`.

**Outputs & Effects:**
- **Before the await (L802-L809):** `take_unbond` removes the entry (and the owner's list if empty), `unbonding_total -= assets`, `holdings -= assets`. Borrow ends at L809.
- **Await (L810):** `push_underlying(underlying, owner, entry.assets)`.
- **After, `Ok`:** `Claimed{owner, id, assets}` event (L812), returns `assets`. No writes.
- **After, `Err`:** `restore_unbond` (L816) puts the entry back (sorted by id), restores both totals; returns the error. No event.

**Block-by-Block:**

```rust
// L802-L809
ensure_gas(GAS_ONE_CALL)?;
let (owner, underlying, entry) = {
    let mut s = self.state.borrow_mut();
    let now = now_ms();
    let owner = s.actor_for(Syscall::message_source(), SessionAction::Claim, now)?;
    let e = s.take_unbond(owner, id, now)?;
    (owner, s.underlying, e)
};
```
- **What:** Gas, actor, take.
- **Why here:** Removing the entry before the await is what makes a second `claim(id)` during the window fail with `UnbondNotFound` (L462) rather than pay twice.
- **Establishes:** `entry.assets <= holdings` (L464), entry no longer claimable, `distributable` unchanged (both terms dropped together).
- **Depended on by:** L810-L819.

```rust
// L810
match push_underlying(underlying, owner, entry.assets).await {
```
- **What an interleaved message can observe between L810 and L811:** the entry absent from `unbonds(owner)`; `unbonding_total` and `holdings` both lower by `assets`; the same rate as before. What it can change: new unbond entries for the same owner get fresh ids (L453), so `restore_unbond`'s re-insert cannot collide; `pause` does not block the restore (no gate in L474-L480); `revoke_session` is irrelevant (owner captured); `collect_fees`/`redeem`/`deposit`/`fund_rewards` move other fields and compose additively with the restore. Token-side pause → `TokenRejected` → restore.
- **Establishes (on `Ok`):** the token paid `assets` to `owner`.

```rust
// L811-L818
Ok(()) => { self.emit_event(VaultEvent::Claimed { owner, id, assets: entry.assets }).expect("event"); Ok(entry.assets) }
Err(e) => { self.state.borrow_mut().restore_unbond(owner, entry); Err(e) }
```
- **What:** Confirm or restore.
- **Assumes:** `Err` means unpaid (timeout caveat as in `vault-push_underlying.md`). `emit_event` trap at L812 discards nothing but the event (no writes in that segment). There is no event after `restore_unbond`, so a trap cannot undo the restore.
- **Depended on by:** the reply; `unbonds` query.

**Cross-Function Dependencies:**
- Callees: `ensure_gas`, `actor_for`, `take_unbond`, `push_underlying`, `restore_unbond`.
- Callers: any actor; a session key with `Claim`. The payout always goes to `owner`, never to the key (L810 uses `owner`; doc L98).
- Shared state: `unbonds`, `unbonding_total`, `holdings`.
- Invariant couplings: accounting bound preserved on every path. A paused vault blocks claims (L460), so matured unbonds are frozen while paused.

**Open Questions:**
- same timeout question.
