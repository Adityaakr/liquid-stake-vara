# arith-1: Constructor accepts `instant_fee_bps` above 100%, so `assets - fee` underflows and panics every instant exit

**Location**
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:188` (`PoolState::new`, no bound on `instant_fee_bps`)
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:275-276` (`preview_unstake`: `fee = assets*bps/BPS`, `net: assets - fee`)
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:208` (`VaultState::new`)
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:296-297` (`preview_redeem`)
- Contrast: `set_config` at pool `:464` / vault `:506` does enforce `instant_fee_bps <= MAX_FEE_BPS (1_000)`.

**Bug class** ARITHOFL (unchecked U256 subtraction) + missing input validation on a deploy-time parameter.

**Severity** Low

**Description**
`MAX_FEE_BPS = 1_000` is only enforced in `set_config`. The program constructor stores whatever `u32` the deployer passes. The fee formula then computes `fee = assets * fee_bps / 10_000` and `net = assets - fee` with the plain `Sub` operator. The `uint` crate (`primitive-types` U256) implements `Sub` with `panic_on_overflow!`, which panics unconditionally, independent of the Cargo `overflow-checks` profile (the sails-generated wasm profile sets only `opt-level = "z"`, `lto = "fat"`).

Two regimes:
1. `10_000 < fee_bps`: `fee > assets`, so `assets - fee` panics. Every `unstake` / `redeem` message and every `preview_unstake` / `preview_redeem` / `info`-adjacent query that reaches the formula traps. `request_unbond` does not use the fee and still works.
2. `1_000 < fee_bps <= 10_000`: no panic, but the documented 10% cap is silently bypassed; a deployer can take up to 100% of an instant exit (`fee_bps == 10_000` gives `net == 0`, which `unstake` rejects with `ZeroAmount`).

**Concrete numbers**
- Deploy with `instant_fee_bps = 20_000`. User holds 100 VARA worth: `assets = 1e14` planck. `fee = 1e14 * 20_000 / 10_000 = 2e14`. `assets - fee = 1e14 - 2e14` -> U256 underflow -> `panic!("arithmetic operation overflow")`. The whole message fails; the user cannot instant-exit until the admin calls `set_config` with a legal fee.
- Deploy with `instant_fee_bps = 9_000`: user exits 100 VARA worth, receives `1e14 - 9e13 = 1e13` (10 VARA), 90% taken as "fee", no error anywhere.
- Deploy with `instant_fee_bps = u32::MAX = 4_294_967_295`: `fee = assets * 4.29e9 / 1e4 = assets * 429_496`; no U256 overflow (max `1e22 * 4.3e9 = 4.3e31`), only the underflow panic above.

**Impact**
Deploy-time misconfiguration (typo, or wrong unit, e.g. passing percent*100*100) bricks the instant-exit path with a panic instead of a clean `BadConfig`, and the code's own fee cap can be bypassed at deploy time. Funds are not lost (unbond path unaffected; admin can repair with `set_config`). Trust model: the deployer is the admin, so this is a robustness/validation issue rather than a theft vector.

**Confidence** High (formula and `uint` panic semantics verified in source).

**Suggested fix**
Validate in the constructor exactly as `set_config` does (reject or clamp `instant_fee_bps > MAX_FEE_BPS`; the vesting period is already clamped there, so clamping the fee is consistent). Independently, compute `net` with `assets.saturating_sub(fee)` or `checked_sub(...).ok_or(PoolError::Overflow)` so an out-of-range fee can never turn into a trap.
