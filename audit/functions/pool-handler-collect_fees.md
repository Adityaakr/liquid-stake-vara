## `Pool::collect_fees` in programs/vara-pool/app/src/lib.rs (L830-L856)

**Purpose:** Admin sends the whole `fees_accrued` bucket (instant-exit fees plus swept orphans) to `to`, out of `reserve`. The compensation logic lives inline rather than in `PoolState`.

**Inputs & Assumptions:**
- `to` (ActorId): untrusted parameter, chosen by the admin; may be any actor including a program.
- Implicit: `message_source` (L832) must equal `admin` (L835, `ensure_admin` L459-L461).
- Preconditions:
  - `fees_accrued <= reserve` (L839): guaranteed by the reserve-cover invariant; checked anyway.
  - Not gated by `paused`.

**Outputs & Effects:**
- Under the borrow (L833-L843): `ensure_admin`, `to != 0`, `amount = fees_accrued`, `amount > 0`, `amount <= reserve`, then `fees_accrued = 0`, `reserve -= amount` (panicking `Sub`, bounded by L839). Early `return Err(...)` inside the block (L836, L838, L839) happens before any write.
- `pay_out(to, amount)` (L844). `Ok`: emit `FeesCollected` (L846), reply `Ok(amount)`. `Err`: `fees_accrued = amount` (assignment, not addition, L851), `reserve += amount` saturating (L852), reply `Err`.
- Postcondition on `Ok`: `fees_accrued == 0`; reserve-cover invariant preserved.

**Block-by-Block:**

```rust
// L834-L842
s.ensure_admin(caller)?;
if to.is_zero() { return Err(PoolError::ZeroAddress); }
let amount = s.fees_accrued;
if amount.is_zero() { return Err(PoolError::ZeroAmount); }
if amount > s.reserve { return Err(PoolError::InsufficientReserve { available: s.reserve }); }
s.fees_accrued = U256::zero();
s.reserve -= amount;
```
- **What:** Authorise, validate, debit.
- **Why here:** Debit before the send, restore after failure: same shape as `unstake`/`claim`. The `return` statements inside the block drop the `RefMut` normally.
- **Establishes:** `amount` is the full bucket; there is no partial collection.

```rust
// L849-L853
Err(e) => { let mut s = self.state.borrow_mut(); s.fees_accrued = amount; s.reserve = s.reserve.saturating_add(amount); Err(e) }
```
- **What:** Restore.
- **Assumes:** nothing wrote `fees_accrued` between L840 and L851: true, `pay_out` is synchronous and touches no state. Assignment rather than `saturating_add` is therefore exact.

**Cross-Function Dependencies:**
- Callee `ensure_admin` (internal, L459-L461): equality with `admin`, which `transfer_admin` (L817-L827) can change to any non-zero actor.
- Callee `pay_out` -> `send_bytes_with_gas` (external-source-available): same queued-not-delivered and ED/balance semantics as the other payouts. `to` is admin-chosen; if it is a program, the 0-gas message's fate is runtime-defined.
- Callee `emit_event`: trap on failure after the send is queued.
- Callers: dispatcher only.
- Shared state: `fees_accrued`, `reserve`.
- Invariant couplings: fees are the sink for `sweep_orphans`; collecting them during an empty spell takes only what was swept at the last stake (see `sweep_orphans`). Rounding dust from exits is not in `fees_accrued`; it stays in `distributable` until the pool empties and the next stake sweeps it.

**Open Questions:**
- unclear; whether `collect_fees` is meant to work while paused (it does; L833-L843 has no `ensure_live`).
