# panic-2: `revert_redeem` saturates `fees_accrued`, so a `collect_fees` that lands between `begin_redeem` and the failed payout leaks the redeem fee out of the vault

- **Location**: `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:415` (`self.fees_accrued = self.fees_accrued.saturating_sub(p.fee)` in `revert_redeem`), reached from `Vault::redeem` `:761-784` (compensation after `push_underlying(...).await` fails); the interleaving partner is `Vault::collect_fees` `:897-917` / `take_fees` `:483-490`
- **Bug class**: saturating arithmetic hiding a wrong result (ARITHOFL, release-silent) + async interleaving across the `.await` in `redeem`
- **Severity**: Low
- **Confidence**: High (on the arithmetic path); trigger needs a failing outbound token transfer plus an admin `collect_fees` in the window

## Description

`begin_redeem` (pre-await) burns the shares, moves `p.fee` into `fees_accrued` and takes
`p.net` out of `holdings`. The state is persisted at the `.await` on `push_underlying`.
While the vault waits for the token's reply, other messages run. If the admin's
`collect_fees` executes in that window, `take_fees` pays out *all* of `fees_accrued`,
including the `p.fee` that belongs to a redeem that has not finished yet, and sets
`fees_accrued = 0`.

If the redeem's payout then fails (`TokenRejected` / `TokenCall`: underlying paused, the
token call timing out, or the transfer replying `false`), `revert_redeem` runs:

```rust
let _ = self.mint_shares(owner, shares);                        // shares come back
self.fees_accrued = self.fees_accrued.saturating_sub(p.fee);    // 0 - fee => 0, silently
self.holdings = self.holdings.saturating_add(p.net);            // net comes back
```

The saturation hides that `fees_accrued` should now be *negative* by `p.fee`: the fee was
already sent to the admin, but the shares it was charged for are restored to the owner.
Net effect: `holdings` is short by `p.fee` relative to what the shares represent, so
`distributable` and the rate drop for every holder.

Ordering check of the other combinations: collect-fails/redeem-fails restores both amounts
additively (`restore_fees` uses `saturating_add`) and is consistent; only
collect-succeeds/redeem-fails leaks. The pool (`vara-pool`) is synchronous, so its
`revert_unstake` (`vara-pool/app/src/lib.rs:388`) has the same shape but no interleaving
window and is not affected.

## Concrete trigger

1. Underlying token is paused (or the vault's `TOKEN_CALL_GAS` call times out).
2. User A calls `redeem(shares)`; `begin_redeem` books `fee` into `fees_accrued`, message waits.
3. Admin calls `collect_fees(to)` (its own token push may still succeed if the token was
   only rejecting the user path, or it was sent before the pause took effect); `fees_accrued`
   including A's fee is paid out.
4. A's `push_underlying` reply arrives as an error; `revert_redeem` saturates to 0.
5. `holdings` is now `fee` lower than what shares back; rate for all holders drops.

## Impact

Value leak bounded by the instant fee (<= 10%) of each redeem that fails while a
`collect_fees` interleaves; socialised across all share holders. No trap, no stuck funds.

## Suggested fix

Track in-flight redeems separately: keep `p.fee` in a `pending_fees` bucket until the payout
is confirmed, and only then move it into `fees_accrued` (or exclude in-flight fees from
`take_fees`). Alternatively make `revert_redeem` return `Result` and, when
`fees_accrued < p.fee`, charge the shortfall against `holdings` explicitly rather than
saturating it away.
