## `PoolState::stake` in programs/vara-pool/app/src/lib.rs (L357-L367)

**Purpose:** Turn `assets` of VARA (already received with the message by the caller) into shares for `owner`, and record the VARA in `reserve`. It is the only path that increases `total_shares` other than `revert_unstake`, and one of two that increase `reserve` (the other is `fund`).

**Inputs & Assumptions:**
- `owner` (ActorId): who receives the shares. Trust: trusted, resolved by `actor_for` in the handler (L687): the sender itself or the owner of the sender's accepted session.
- `assets` (U256): the message value (L682-L683). Trust: untrusted amount, but it is real value the runtime delivered with the message.
- `now_ms` (u64): clock.
- Implicit state read: `paused`, `total_shares`, `base_rate`, buckets via `distributable`.
- Preconditions:
  - The VARA equal to `assets` is in the program's balance: established by the handler reading `Syscall::message_value()` (L682) and by Gear delivering value with the message; nothing in this file could check it.
  - Not paused (L358).

**Outputs & Effects:**
- Writes: `fees_accrued` (via `sweep_orphans`, only when `total_shares == 0`, L361), `balances[owner]` and `total_shares` (via `mint_shares`, L364), `reserve` (L365).
- Returns `Ok(shares)`; errors `Paused` L358, `ZeroAmount` L359, `BelowMinimum` L360/L363, `Overflow` L362/L364/L365.
- Partial-write paths (persist because `Err` is a normal reply): sweep before L362/L363 errors (see `sweep_orphans`); `mint_shares` before the `reserve` `checked_add` at L365 (shares credited, reserve not) — reachable only if `reserve + assets` overflows U256.

**Block-by-Block:**

```rust
// L358-L360
self.ensure_live()?;
if assets.is_zero() { return Err(PoolError::ZeroAmount); }
if assets < U256::from(MIN_STAKE) { return Err(PoolError::BelowMinimum { min: U256::from(MIN_STAKE) }); }
```
- **What:** Pause gate and amount floor (0.01 VARA, L29-L30).
- **Why here:** Before any write.
- **Establishes:** `assets >= 1e10`. The comment at L29 says this keeps share rounding from producing zero; that holds only while `distributable / total_shares` (the price of one share) is below 1e10 wei per share-unit, i.e. rate below 1e10 * 1e18 / 1e12; L363 is the actual guard.

```rust
// L361-L363
self.sweep_orphans(now_ms);
let shares = self.shares_for(assets, now_ms)?;
if shares.is_zero() { return Err(PoolError::BelowMinimum { min: U256::from(MIN_STAKE) }); }
```
- **What:** Clear ownerless VARA if the pool is empty, price the deposit, refuse a zero mint.
- **Why here:** Sweep before pricing so an empty pool prices at `base_rate` (see `sweep_orphans`, `shares_for`). The zero check protects the depositor (they would otherwise pay `assets` for nothing) and the supply (`mint_shares` would accept zero).
- **Depended on by:** L364.

```rust
// L364-L366
self.mint_shares(owner, shares)?;
self.reserve = self.reserve.checked_add(assets).ok_or(PoolError::Overflow)?;
Ok(shares)
```
- **What:** Credit shares, then book the VARA.
- **Why here:** Shares before reserve: if `mint_shares` fails (`Overflow` at L329 or L294) nothing has been written; if L365 fails, shares exist without backing (see partial-write note).
- **Establishes:** reserve-cover invariant preserved (only `reserve` grew); value per share non-decreasing (rounding in `shares_for`).

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal, L289-L291): `paused == false`.
- Callee `sweep_orphans` (internal, L349-L354): makes `distributable == 0` when `total_shares == 0`.
- Callee `shares_for` (internal, L245-L250): depended on for `shares <= assets * 1e18 / base_rate` on the empty path and pro rata otherwise; returns `Err(Overflow)` on checked-mul failure, `Ok(0)` when the deposit is below one share.
- Callee `mint_shares` (internal, L328-L333): supply `checked_add` first (L329), then balance (L330), then supply stored (L331); on a balance `Overflow` (L294) the supply is untouched.
- Callers: `Pool::stake` L687, which refunds the value on any `Err` (L695) and emits `Transfer(0 -> owner)` and `Staked` on `Ok` (L691-L692).
- Shared state: see `distributable` record.
- Invariant couplings: `base_rate` and `sweep_orphans` decide the price after an empty spell; `MIN_STAKE` and L363 together decide the smallest mint.

**Open Questions:**
- unclear; whether the handler's `assets = U256::from(message_value)` can differ from the value actually retained by the program (the refund path at L695 returns exactly `value`; the success path retains it). No in-file check compares `reserve` with the program balance.
