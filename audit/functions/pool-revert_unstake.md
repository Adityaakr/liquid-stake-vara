## `PoolState::revert_unstake` in programs/vara-pool/app/src/lib.rs (L386-L390)

**Purpose:** Undo the counter changes of a successful `unstake` after the runtime refused the payout (`pay_out` `Err`, L716-L718), so the holder keeps their shares and the pool its VARA.

**Inputs & Assumptions:**
- `owner`, `shares`, `p`: the exact values `unstake` consumed and returned (L707, L717). Trust: trusted; same execution, no intervening code between L707 and L717 other than `pay_out` (synchronous, L672-L674).
- Preconditions:
  - State is exactly as `unstake` left it: holds because the program is synchronous (comment L15-L16) and the only code between the two calls is `pay_out` and a `match` (L710-L717).

**Outputs & Effects:**
- `mint_shares(owner, shares)` (L387) with the error discarded; `fees_accrued -= p.fee` saturating (L388); `reserve += p.net` saturating (L389).
- Does not restore `base_rate` if `burn_shares` overwrote it at L341 (when `shares` was the last of the supply). After the revert `total_shares > 0` again, so `base_rate` is not read until the next true empty spell, at which point `burn_shares` overwrites it anew; the stale value is never consumed on a monotonic path.

**Block-by-Block:**

```rust
// L387
let _ = self.mint_shares(owner, shares);
```
- **What:** Re-credit the shares.
- **Assumes:** cannot fail: `total_shares + shares` equals the pre-`unstake` supply, which fitted (L329); the balance likewise (L294). If it did fail the discard would leave the fee and reserve restored but the shares gone, silently.

```rust
// L388-L389
self.fees_accrued = self.fees_accrued.saturating_sub(p.fee);
self.reserve = self.reserve.saturating_add(p.net);
```
- **What:** Exact inverses of L380-L381.
- **Establishes:** reserve-cover invariant restored to its pre-`unstake` value.

**Cross-Function Dependencies:**
- Callee `mint_shares` (internal, L328-L333).
- Callers: `Pool::unstake` L717 only. The handler returns the `TransferFailed` error after this; no events are emitted on that path.
- Shared state: as `unstake`.

**Open Questions:**
- unclear; whether `pay_out` returning `Err` is the only failure the handler needs to compensate. `send_bytes_with_gas` returning `Ok` means the message was enqueued (gcore msg.rs L1023-L1050), not delivered; a later delivery failure returns value to the program without any call back into this code.
