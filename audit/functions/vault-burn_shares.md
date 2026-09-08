## `VaultState::burn_shares` in programs/vault/app/src/lib.rs (L357-L364)

**Purpose:** Removes `value` shares from `from` and from `total_shares`; when the last share goes it snapshots the pre-burn rate into `base_rate` so an empty vault keeps its price (doc L180-L181, test L1143). Both exit paths (`begin_redeem` L406, `request_unbond` L450) go through it.

**Inputs & Assumptions:**
- `from` (ActorId): the owner resolved by `actor_for`. Trust: semi-trusted (a session key can name its owner, L516-L522).
- `value` (U256): shares from the payload. Trust: untrusted.
- `now_ms` (u64): trusted.
- Precondition: `balance_of(from) >= value`. Checked by the callers (L401, L445) and again here by `debit` L321 (`checked_sub` → mapped to `InsufficientShares` at L360).
- Precondition: `total_shares >= value`. Established by the invariant `sum(balances) == total_shares`: `mint_shares` L350-L352 adds the same amount to both; `burn_shares` L360-L361 removes the same amount from both; `transfer_shares` L330-L331 moves between balances only. `revert_redeem` L414 discards a `mint_shares` error, which is the one place the invariant can drift (only on `Overflow` at L350 or L315).

**Outputs & Effects:**
- State writes: `balances[from]` (L360 via `debit`: removed when zero, L322), `total_shares` (L361), `base_rate` when `total_shares` becomes zero (L362).
- Returns `Err(ZeroAmount)` L358, `Err(InsufficientShares)` L360, `Err(Overflow)` L361 (unreachable while the invariant holds), else `Ok(())`.
- Partial write on the L361 error path: `debit` at L360 has already run when `checked_sub` at L361 fails. Reachable only if `sum(balances) > total_shares`.

**Block-by-Block:**

```rust
// L358-L359
if value.is_zero() { return Err(VaultError::ZeroAmount); }
let rate_before = self.rate_at(now_ms);
```
- **What:** Reject zero, then read the rate *before* touching balances.
- **Why here:** `rate_at` must see the pre-burn `total_shares` and pre-payout `holdings`; both callers call `burn_shares` before adjusting `holdings`/`fees`/`unbonding_total` (L406-L408, L450-L451).
- **Assumes:** the callers' order. `begin_redeem` L406-L408 and `request_unbond` L450-L451 both burn first.
- **Establishes:** `rate_before` = the price the exiting holder received.
- **Depended on by:** L362.

```rust
// L360-L362
self.debit(from, value).map_err(|_| VaultError::InsufficientShares)?;
self.total_shares = self.total_shares.checked_sub(value).ok_or(VaultError::Overflow)?;
if self.total_shares.is_zero() { self.base_rate = rate_before; }
```
- **What:** Burn, then remember the rate if the vault is now empty.
- **Why here:** `base_rate` is only consulted when `empty()` (L261, L267, L275), so writing it only on the transition to zero is enough.
- **Assumes:** `rate_before > 0` so that `shares_for` L268 never divides by zero. `rate_at` returns `base_rate` when already empty (which was non-zero by induction from L219) or `distributable * SCALE / total_shares`, which is zero only if `total_shares > distributable * 1e18`; nothing found that excludes that (see `vault-rate_at.md`).
- **Establishes:** `base_rate` carries the rate across the empty spell.
- **Depended on by:** `shares_for`/`assets_for`/`rate_at` on the empty branch.

**Cross-Function Dependencies:**
- Callee `debit` (internal L320-L324): all paths either write the new balance or return `Err` without writing.
- Callee `rate_at` (internal): see record.
- Callers: `begin_redeem` L406, `request_unbond` L450. Both check the balance first, so the `InsufficientShares` here is a second line. Neither caller checks `total_shares >= value` separately.
- Shared state: `balances`, `total_shares`, `base_rate`. `revert_redeem` L414 is the compensating mint.
- Invariant couplings: `sum(balances) == total_shares` as above. `base_rate` clamp `>= SCALE` exists only in the constructor (L219); the write here is unclamped, so a rate that fell below `SCALE` through dust dilution is preserved as-is.

**Open Questions:**
- none beyond `rate_before > 0`, recorded above.
