## `Vault::collect_fees` in programs/vault/app/src/lib.rs (L897-L917)

**Purpose:** Admin sends the accrued instant-exit fees (and swept orphans, comment L895) to `to`. Fees are zeroed and removed from `holdings` before the await; restored on failure. Exported as `CollectFees(to) -> Result<U256, VaultError>` (IDL L143).

**Inputs & Assumptions:**
- `to` (ActorId): admin-chosen recipient. Trust: admin-trusted; only `is_zero` is rejected (L903) — the token would trap on zero anyway (demo L99).
- Implicit: `message_source()` pre-await (L899) — not via `actor_for` (sessions cannot collect fees); gas via `ensure_gas`.
- Precondition: caller is admin. Established by `ensure_admin` L902 (`admin == who`, L501-L503).

**Outputs & Effects:**
- **Before the await (L898-L906):** `take_fees` writes `fees_accrued = 0`, `holdings -= amount` (L904). Borrow ends at L906.
- **Await (L907):** `push_underlying(underlying, to, amount)`.
- **After, `Ok`:** `FeesCollected{to, assets: amount}` event (L909), returns `amount`.
- **After, `Err`:** `restore_fees(amount)` (L913) adds back to both; returns the error.

**Block-by-Block:**

```rust
// L898-L906
ensure_gas(GAS_ONE_CALL)?;
let caller = Syscall::message_source();
let (underlying, amount) = {
    let mut s = self.state.borrow_mut();
    s.ensure_admin(caller)?;
    if to.is_zero() { return Err(VaultError::ZeroAddress); }
    let amount = s.take_fees()?;
    (s.underlying, amount)
};
```
- **What:** Gas, admin gate, zero-recipient gate, take.
- **Why here:** Zeroing `fees_accrued` before the await means a second `collect_fees` in the window gets `ZeroAmount` (L485) rather than paying twice; and fees credited by `redeem`s in the window start from zero and survive the restore (additive L493).
- **Establishes:** `0 < amount <= holdings` (L485-L486).
- **Depended on by:** L907-L916.

```rust
// L907
match push_underlying(underlying, to, amount).await {
```
- **What an interleaved message can observe between L907 and L908:** `fees_accrued == 0` (plus any new fees), `holdings` lower by `amount`, `distributable` unchanged. What it can change: `transfer_admin` (L883) — irrelevant, the admin check already passed; a `redeem` that then fails and reverts will `saturating_sub` its fee from a bucket that no longer contains it (see `vault-revert_redeem.md`); a deposit that sweeps orphans (L370-L375) adds to the bucket; `pause` — no gate on the restore.
- **Establishes (on `Ok`):** `amount` paid to `to`.

```rust
// L908-L915
Ok(()) => { self.emit_event(VaultEvent::FeesCollected { to, assets: amount }).expect("event"); Ok(amount) }
Err(e) => { self.state.borrow_mut().restore_fees(amount); Err(e) }
```
- **What:** Confirm or restore.
- **Assumes:** `Err` means unpaid (timeout caveat). No event follows the restore, so a trap cannot undo it.

**Cross-Function Dependencies:**
- Callees: `ensure_gas`, `ensure_admin`, `take_fees`, `push_underlying`, `restore_fees`.
- Callers: the admin only (`ensure_admin`); `transfer_admin` L883-L893 changes who that is.
- Shared state: `fees_accrued`, `holdings`.
- Invariant couplings: accounting bound preserved. The fee bucket is the sink for both instant-exit fees (L407) and orphaned assets (L373), so `collect_fees` is also how rewards that vested while the vault was empty leave the vault.

**Open Questions:**
- same timeout question.
