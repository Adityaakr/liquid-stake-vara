# Arithmetic cluster: notes on candidates that did not confirm

Scope: `programs/vara-pool/app/src/lib.rs` (P), `programs/vault/app/src/lib.rs` (V), `programs/demo-token/app/src/lib.rs` (T). Prompts applied: arithmetic-overflow, lossy-from-into, nondeterminism, float-edge.

Ground facts used throughout:
- `U256` is `uint 0.10.0` via `primitive-types`. `+ - *` call `panic_on_overflow!`, which panics unconditionally (not gated on `overflow-checks`); `/` asserts `"division by zero"`. `saturating_mul` returns `U256::MAX` on overflow. Verified in `~/.cargo/registry/src/.../uint-0.10.0/src/uint.rs:351-360, 952-958, 1137`.
- The sails-generated wasm profile has `opt-level = "z"`, `lto = "fat"`, no `overflow-checks`, so native `u64` ops wrap silently in the deployed binary.
- Magnitude envelope. VARA: `1e9` VARA supply = `1e21` planck, generous ceiling `1e22`; message value is `u128` (`<= 3.4e38`). `SCALE = 1e18`. `U256::MAX ~= 1.16e77`. Timestamps `~1.7e12` ms; periods `<= 3.15e10` ms.

## Not reachable at realistic magnitudes (kept for the record)

1. `locked_rewards`: `vest_amount.saturating_mul(left) / span` (P:225, V:246). Saturation needs `vest_amount > 1.16e77 / 3.15e10 = 3.7e66`. Pool max `1e22 * 3.15e10 = 3e32`. If it did saturate the result `MAX/span` exceeds `vest_amount`, `distributable` saturates to 0, `empty()` flips true and the pool prices at `base_rate` while holding shares: a real corruption shape, but 44 orders of magnitude out of reach for VARA. Vault: only a token with `> 3.7e66` base units (demo-token admin/minter can mint arbitrary U256, but that is a privileged action on a demo asset).
2. `rate_at`: `distributable.saturating_mul(SCALE) / total_shares` (P:241, V:262). Saturates above `1.16e59` distributable. Pool max `1e40`.
3. `shares_for` / `assets_for` `checked_mul` (P:247-257, V:268-278). Return `Overflow` cleanly. Pool worst case `assets(3.4e38 u128) * total_shares(1e22) = 3.4e60 < 1.16e77`. Vault would need `assets * total_shares > 1.16e77`, i.e. roughly `1e38.5` each; no real token approaches this. Note the vault error path: `settle_deposit` runs after tokens were pulled, so an `Overflow` there would strand tokens uncounted in `holdings`; unreachable for the same reason (belongs to the async cluster if magnitudes ever change).
4. `apy_bps` numerator `vest_amount * YEAR_MS(3.1536e10) * BPS(1e4)` (P:269, V:290). Saturates above `3.7e62`. Pool max `3e36`. `YEAR_SECS * 1000` is a const-folded u64 (`3.15e10`, fits). Denominator `span * d`: both `>= 1` by the guards, never zero. Result clamped to `u32::MAX` before `low_u32`; display only.
5. `fund` weighted average `(remaining*left + assets*period) / total` then `.max(1).low_u64()` (P:405-407, V:432-434). `total > 0` because `assets != 0`. Result is a weighted mean of two u64-range values so it is `<= max(left, period) <= 3.15e10` and `low_u64` is exact. `.max(U256::one())` is dead code: `period >= 3.6e6` makes the mean `>= 1` whenever `assets >= 1`. `span` in `locked_rewards` is therefore always `>= 1` (`vest_end = now + time_left`, `time_left >= 1`), so the `/ span` is never a zero divide. If `now_ms.saturating_add` ever saturated, `span = MAX - start > 0` still.
6. `next_unbond_id += 1` (P:426, V:453): u64 wrap after `1.8e19` unbonds. FP.
7. `as_value` (P:666-668): clamps `U256 -> u128` at `u128::MAX`. `reserve` is a sum of u128 message values, so exceeding u128 needs `3.4e26` VARA. FP.
8. `U256::from(Syscall::message_value())` (P:683, 764): `u128 -> U256`, lossless.
9. `duration_secs.saturating_mul(1000)` (P:487, V:529): bounded by `MAX_SESSION_SECS` check first. Fine.
10. `unbond_period_ms / 1000`, `vesting_period_ms / 1000`, `faucet_cooldown_ms / 1000` (P:522-523, V:565-566, T:452): stored as exact `secs*1000` so the division is exact (except after the saturation in arith-2).
11. Demo token `last.saturating_add(faucet_cooldown_ms)` (T:142, 152): a cooldown above `u64::MAX - now` pins `next_claim_at` at `u64::MAX` and disables the faucet for prior claimers; admin-only, cosmetic.

## Rounding direction (all divisions audited)

- `shares_for` rounds shares down (pool's favour). `assets_for` rounds assets down (pool's favour). `rate_at` rounds down; only used for display, events and to capture `base_rate` when the pool empties, where the downward rounding favours the *next* staker by `< 1e-18` relative. Fine.
- `fee = assets * bps / BPS` rounds down (user's favour, `< 1` planck per exit). Fee-free exits require chunks of `< BPS/bps = 334` planck (`3.3e-10` VARA) per message; exiting 100 VARA that way needs `3e11` messages. Not viable.
- `locked_rewards` rounds down, so `distributable` rounds up: `< 1` planck of a tranche is released one block early. Each `fund` re-folds `remaining` rounded down, releasing `< 1` planck per call; dust-funding every block to farm this yields `< 1` planck per block.
- Rate monotonicity: every path either rounds in the pool's favour or adds assets, so `distributable >= total_shares` (rate `>= 1` planck/share) holds from the first stake, which makes the `distributable.is_zero()` clause of `empty()` unreachable while shares exist (P:234-236, V:255-257). The vault's `settle_deposit(...).max(U256::one())` (V:390) could in theory mint 1 share for less than a share's worth if the rate crossed above `assets` between `check_deposit` and the token reply, but `check_deposit` has already required `shares >= 1` at the pre-await rate and the rate cannot move by a full share's worth of a `>= 1000`-unit deposit in one block; gain bounded by `< 1` share. Note the asymmetry with the pool, which has no `.max(1)`.

## Zero divisors

- `/ self.base_rate` (P:247, V:268): `base_rate >= SCALE` by the constructor clamp and only ever overwritten with `rate_at`, which is `>= SCALE` by monotonicity. Non-zero.
- `/ self.distributable(now)` (P:249, V:270) and `/ self.total_shares` (P:241, 257; V:262, 278): guarded by `empty()`.
- `/ span`, `/ (span*d)`, `/ total`: see items 4-5 above.
- `/ U256::from(BPS)`: constant.

## ERC-4626 first-depositor / donation inflation

`reserve`/`holdings` are internal accounting, so a plain value transfer or token transfer to the program does not move the rate; the only donation path is `fund_rewards`, which vests over `>= 1` hour. Attacker stakes `MIN_STAKE = 1e10`, funds `X`, waits a full vesting period: victim's rounding loss is `<= 1` share `= (1e10 + X)/1e10` planck. To steal 1 VARA (`1e12`) of rounding the attacker must donate `X = 1e22` planck (`1e10` VARA). Not exploitable. Emptying the pool to freeze a huge `base_rate` has the same economics. `sweep_orphans` correctly sends anything left in an empty pool to fees rather than to the next staker.

## Distributable saturating-sub chain (P:230, V:251)

`reserve - fees - unbonding - locked` uses `saturating_sub` three times, which would mask an accounting error by clamping at 0 (and makes the result depend on subtraction order once it clamps). Checked every mutation pair: `unstake`/`revert_unstake`, `take_unbond`/`restore_unbond`, `collect_fees` success/failure, `take_fees`/`restore_fees`, `sweep_orphans`. Each keeps `fees + unbonding + locked <= reserve`. No path found that breaks it; keep as an invariant to assert in tests.

## Config validation gaps adjacent to arith-1 / arith-2

- `initial_rate: U256` is unbounded above (P:198, V:219). With `initial_rate = 1e30` the first stake of `1e22` planck gets `1e22 * 1e18 / 1e30 = 1e10` shares (fine), but at `1e41` every stake computes 0 shares and is rejected with `BelowMinimum`; the program is unusable from deployment and only a redeploy fixes it. `assets_for` on the empty branch would return `Overflow` for `shares * base_rate > 1.16e77`. Deploy-time, admin-trusted: Info.
- Constructor clamps `vesting_period_secs` into range while `set_config` rejects out-of-range values; harmless inconsistency.

## Nondeterminism / floats

All maps are `BTreeMap`/`BTreeSet` (`sails_rs::collections`); `minters()` iterates a `BTreeSet`; no `HashMap`, no `f32`/`f64`, no pointer values in state or events. Nothing to report.

## Lossy casts

`low_u32` (clamped first), `low_u64` (bounded, item 5), `low_u128` (clamped first), `U256::from(u32|u64|u128)` widening only. `as i64` appears only in test assertions. No security-relevant truncation beyond arith-2's `saturating_mul`.
