## `VaultState::revert_redeem` in programs/vault/app/src/lib.rs (L413-L417)

**Purpose:** Compensation for `begin_redeem` when the outbound `transfer` failed (doc rule 2 at L17): give the shares back, take the fee back, put the net payout back into `holdings`. Runs post-await in `Vault::redeem` L778, in a fresh execution with whatever state other messages left.

**Inputs & Assumptions:**
- `owner`, `shares` (the same values passed to `begin_redeem`, captured before the await at L763-L770). Trust: internal.
- `p` (&RedeemPreview): the preview `begin_redeem` returned. Trust: internal.
- Precondition: the token did **not** move `p.net` to `owner`. Established by `push_underlying` returning `Err` only on `Ok(Ok(false))`, `Ok(Err(TokenError))` (the token trapped, so its state was discarded) or a transport `Err`. Among transport errors, `Timeout` (gstd `msg/async.rs` L42-L46, default 100 blocks per `config.rs` L73) does not establish that the transfer did not happen; nothing found that excludes a late-but-successful transfer.

**Outputs & Effects:**
- State writes: `balances[owner]`, `total_shares` via `mint_shares` L414 (error discarded); `fees_accrued -= p.fee` saturating L415; `holdings += p.net` saturating L416.
- Returns nothing; cannot fail.

**Block-by-Block:**

```rust
// L414
let _ = self.mint_shares(owner, shares);
```
- **What:** Re-mint the burnt shares.
- **Why here:** First, so the holder is whole before the accounting changes.
- **Assumes:** `mint_shares` succeeds. It fails only on `Overflow` (L350 or L315); the discarded error would leave `total_shares`/`balances` un-restored while L415-L416 still restore the assets, breaking `sum(balances) == total_shares` in the direction `total_shares < ` nothing (both unchanged) — the shares are simply lost while the assets come back to the pool. Nothing found that makes the overflow unreachable other than the size of `U256`.
- **Establishes:** the holder's balance is back (on the success path).
- **Depended on by:** the `Transfer` event at L779.

```rust
// L415-L416
self.fees_accrued = self.fees_accrued.saturating_sub(p.fee);
self.holdings = self.holdings.saturating_add(p.net);
```
- **What:** Undo the fee credit and the payout debit.
- **Why here:** Saturating because state may have moved during the await: `collect_fees` may have paid `fees_accrued` (including this `p.fee`) out already. In that case `fees_accrued` floors at zero and `holdings` still ends up equal to the tokens the vault physically holds (fee paid, net not paid); the fee is then borne by the pool since the shares are back but `distributable` is lower by `p.fee`.
- **Assumes:** `holdings + p.net` does not saturate (it was `holdings + p.net` before `begin_redeem`).
- **Establishes:** `holdings` matches tokens held under the precondition above.
- **Depended on by:** all subsequent pricing.

**Cross-Function Dependencies:**
- Callee `mint_shares` (internal L349-L354): error discarded, see above.
- Callers: `Vault::redeem` L778 only, on the `Err` arm of `push_underlying`.
- Shared state: `fees_accrued` with `collect_fees`/`take_fees`; `holdings` with everything.
- Invariant couplings: `burn_shares` may have written `base_rate` (L362) when this redeem emptied the vault; the revert does not restore `base_rate`. Since shares come back, `empty()` is false again and `base_rate` is not consulted until the next emptying, which overwrites it (L362). `sum(balances) == total_shares` preserved except on the discarded overflow.

**Open Questions:**
- unclear; need to inspect Gear 1.10 runtime behaviour when a reply arrives after `wait_up_to` expired (gstd `locks.rs` L246-L256 reports the timeout; `signals.rs` L68-L80 records a late reply against a future that already completed). Whether the late token transfer can have succeeded while the vault treats it as failed decides whether this function can double-credit.
