## `VaultState::check_deposit` in programs/vault/app/src/lib.rs (L378-L385)

**Purpose:** The pre-await validation of a deposit (doc rule 1 at L15): paused, zero, below minimum, or too small to mint a share. It returns the shares a deposit *would* mint now, but `Vault::deposit` discards that number (L744) and re-prices after the await (L751).

**Inputs & Assumptions:**
- `assets` (U256): from the payload. Trust: untrusted.
- `now_ms` (u64): trusted.
- Implicit state read: `paused` (via `ensure_live` L310-L312), pricing state via `shares_for`.
- Precondition: none.

**Outputs & Effects:**
- No state writes. Returns `Err(Paused)` L379, `Err(ZeroAmount)` L380, `Err(BelowMinimum{min})` L381 or L383, `Err(Overflow)` from `shares_for` L382, else `Ok(shares)`.

**Block-by-Block:**

```rust
// L379-L381
self.ensure_live()?;
if assets.is_zero() { return Err(VaultError::ZeroAmount); }
if assets < U256::from(MIN_DEPOSIT) { return Err(VaultError::BelowMinimum { min: U256::from(MIN_DEPOSIT) }); }
```
- **What:** Pause gate and a 1,000 base-unit floor (L37-L38).
- **Why here:** Before any token movement; all of these are cheap and final for this message.
- **Assumes:** `MIN_DEPOSIT = 1_000` is meaningful in the token's decimals — it is a constant in base units regardless of `decimals` (L175), so for a 6-decimal token it is 0.001 units and for an 18-decimal token it is 1e-15 units.
- **Establishes:** `assets >= 1_000` for L382.
- **Depended on by:** L382-L383 only; `settle_deposit` does not re-check any of these.

```rust
// L382-L384
let shares = self.shares_for(assets, now_ms)?;
if shares.is_zero() { return Err(VaultError::BelowMinimum { min: U256::from(MIN_DEPOSIT) }); }
Ok(shares)
```
- **What:** Reject a deposit that would round to zero shares at the current price.
- **Why here:** The comment at L37 says the minimum exists "so share rounding can never produce zero"; this is the runtime check for the case where the rate is high enough that 1,000 base units still round to zero.
- **Assumes:** the price will not move enough during the await to change the answer. It can: `settle_deposit` re-prices at L390 with post-await state and floors at one share rather than rejecting.
- **Establishes:** nothing that survives the await.
- **Depended on by:** nothing; the return value is dropped at L744.

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal L310-L312), `shares_for` (internal): see records.
- Callers: `Vault::deposit` L744 only. `fund_rewards` does **not** call it (L829-L833 checks only `assets.is_zero()`), so funding is possible while paused and below `MIN_DEPOSIT`.
- Shared state: reads `paused`, which `pause`/`resume` (L859-L880) write; a pause landing during the await is not seen by `settle_deposit`.
- Invariant couplings: none written.

**Open Questions:**
- none.
