# arith-2: `unbond_period_secs` is unbounded; `saturating_mul(1000)` / `saturating_add` pin `claimable_at` at `u64::MAX` and there is no cancel path

**Location**
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:200` (`unbond_period_ms: unbond_period_secs.saturating_mul(1000)` in `PoolState::new`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:463-469` (`set_config` validates fee and vesting period but not `unbond_period_secs`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:425` (`claimable_at: now_ms.saturating_add(self.unbond_period_ms)`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:436` (`if now_ms < list[idx].claimable_at { return Err(UnbondNotReady) }`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:522` (`info().unbond_period_secs = unbond_period_ms / 1000`)
- Vault mirrors: `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:221`, `:505-511`, `:452`, `:463`, `:565`.

**Bug class** ARITHOFL (u64 saturation used as a silent clamp) + missing upper bound on config.

**Severity** Low (requires admin/deployer action; effect on users is a permanent lock)

**Description**
`MIN_VESTING_SECS..=MAX_VESTING_SECS` and `MAX_FEE_BPS` are enforced, `MAX_SESSION_SECS` is enforced, but the unbond period has no maximum anywhere. The seconds-to-milliseconds conversion uses `saturating_mul(1000)`, so any value above `u64::MAX / 1000 = 18_446_744_073_709_551` becomes `u64::MAX` milliseconds. `request_unbond` then computes `claimable_at = now_ms.saturating_add(u64::MAX) = u64::MAX`. `take_unbond` requires `now_ms >= claimable_at`, which can never hold, and the program has no admin or user function that cancels or re-prices an unbond entry. The burned shares are gone and `entry.assets` stays parked in `unbonding_total` forever, excluded from `distributable`, so nobody can ever withdraw it.

Saturation is not even needed: any absurd but non-saturating value (say `1e12` seconds, about 31,000 years) has the same practical effect, and `saturating_mul` only hides the arithmetic edge.

**Concrete numbers**
- Admin calls `set_config(30, 18_446_744_073_709_552, 604_800)`. `unbond_period_ms = 18_446_744_073_709_552 * 1000` overflows u64 -> saturates to `18_446_744_073_709_551_615`. `info().unbond_period_secs` reports `18_446_744_073_709_551` (not what was set).
- User with 1,000 VARA (`1e15` planck) calls `request_unbond(all shares)` at `now_ms = 1_757_300_000_000`: `claimable_at = 1_757_300_000_000 + u64::MAX -> u64::MAX`. `claim(id)` returns `UnbondNotReady { claimable_at: 18446744073709551615 }` on every block until the heat death of the chain. The 1e15 planck remain in `reserve` but are subtracted from `distributable` via `unbonding_total`, so neither the user nor the other holders nor `collect_fees` (which is capped by `reserve` but only pays `fees_accrued`) can reach it.
- Entries created before the config change keep their original `claimable_at`, so the lock only hits requests made after the change.

**Impact**
An admin mistake (or a compromised/malicious admin) turns every subsequent unbond into an irrecoverable lock, with no on-chain recovery even for the admin. Within the stated trust model (admin trusted) this is Low, but the absence of an upper bound is inconsistent with the rest of `set_config` and with the accompanying `saturating_*` clamp, which makes the failure silent rather than rejected.

**Confidence** High.

**Suggested fix**
Add `MAX_UNBOND_SECS` (e.g. 28 days, comfortably above Vara's own 7-day unbonding) and reject it in both the constructor and `set_config` with `BadConfig`, using `checked_mul(1000).ok_or(BadConfig)` instead of `saturating_mul`. Optionally add an admin-only `expedite_unbond`/`cancel_unbond` for recovery.
