## `PoolState::set_config` in programs/vara-pool/app/src/lib.rs (L463-L469)

**Purpose:** Replace the three tunables: instant fee, unbond period, vesting period. Admin-only at the handler (L785).

**Inputs & Assumptions:**
- `instant_fee_bps` (u32), `unbond_period_secs` (u64), `vesting_period_secs` (u64): from the admin's message (L781). Trust: semi-trusted (admin).
- Preconditions: the caller is the admin: enforced by the handler's `ensure_admin` (L785, L459-L461), not here.

**Outputs & Effects:**
- Writes `instant_fee_bps`, `unbond_period_ms`, `vesting_period_ms` (L465-L467). Error `BadConfig` (L464) before any write.
- Postconditions: `instant_fee_bps <= 1000` (10%), `vesting_period_secs` in `[3600, 365 days]`; `unbond_period_ms` any `u64` (saturating multiply).

**Block-by-Block:**

```rust
// L464
if instant_fee_bps > MAX_FEE_BPS || !(MIN_VESTING_SECS..=MAX_VESTING_SECS).contains(&vesting_period_secs) { return Err(PoolError::BadConfig); }
```
- **What:** Bound the fee and the vesting period; the unbond period is not bounded in either direction.
- **Establishes:** the `fee <= assets` precondition of `preview_unstake` L276 for all post-deployment values. The constructor path (L199) does not apply this check, and L201 clamps rather than rejects the vesting period.

```rust
// L465-L467
self.instant_fee_bps = instant_fee_bps;
self.unbond_period_ms = unbond_period_secs.saturating_mul(1000);
self.vesting_period_ms = vesting_period_secs.saturating_mul(1000);
```
- **What:** Store in ms.
- **Depended on by:** `preview_unstake` L275 (fee, applies to all future unstakes immediately), `request_unbond` L425 (period, applies to new requests only; existing entries keep their `claimable_at`), `fund` L399 (vesting, applies to the next `fund`; the running tranche keeps its `vest_end_ms`).

**Cross-Function Dependencies:**
- No callees.
- Callers: `Pool::set_config` L786, after `ensure_admin` L785; emits `ConfigChanged` L788.
- Shared state: the three fields; read by the functions listed above.
- Invariant couplings: `unbond_period_secs == 0` makes `request_unbond` + `claim` a fee-free instant exit (see `request_unbond`); `instant_fee_bps == 0` makes `unstake` fee-free. Both are reachable by the admin alone, with no timelock in this file.

**Open Questions:**
- none.
