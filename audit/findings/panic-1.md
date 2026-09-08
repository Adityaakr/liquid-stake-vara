# panic-1: Constructor accepts `instant_fee_bps > 10_000`, turning every instant exit into a `U256` underflow panic

- **Location**
  - `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:220` (`VaultState::new` stores `instant_fee_bps` unchecked), `:297` (`net: assets - fee`), `:1018` (`Program::new`)
  - `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:199` (`PoolState::new`), `:276` (`net: assets - fee`), `:957` (`Program::new`)
- **Bug class**: ARITHOFL (unconditional `U256::sub` panic, `uint-0.9.5` `panic_on_overflow!` fires regardless of `overflow-checks`) / missing constructor validation
- **Severity**: Low
- **Confidence**: High

## Description

`set_config` enforces `instant_fee_bps <= MAX_FEE_BPS (1_000)`, but the constructor path
(`Program::new` -> `VaultState::new` / `PoolState::new`) stores the deployer-supplied
`instant_fee_bps` without any bound. `preview_redeem` / `preview_unstake` compute

```rust
let fee = assets.saturating_mul(U256::from(self.instant_fee_bps)) / U256::from(BPS);
Ok(RedeemPreview { assets, fee, net: assets - fee })
```

With `instant_fee_bps > 10_000` the fee exceeds `assets` and the plain `-` on `U256`
traps with `arithmetic operation overflow` (checked in
`~/.cargo/registry/src/*/uint-0.9.5/src/uint.rs:351`; the macro is unconditional, the
absence of `overflow-checks` in every `Cargo.toml` does not help here).

Path: `Vault::redeem` -> `begin_redeem` -> `preview_redeem` -> `assets - fee` (pre-await, so
the message reverts cleanly) and the `Vault::preview_redeem` / `Pool::preview_unstake`
queries, which `unwrap_or` only the `Err` branch, not a trap. Values in `1_000 < bps <= 10_000`
do not panic but silently exceed the documented 10% cap.

## Concrete trigger

Deploy with `instant_fee_bps = 20_000` (or any value above 10_000). Any `redeem`/`unstake`
of shares with a non-zero `assets` value traps; `preview_redeem`/`preview_unstake` queries
trap. `request_unbond` is unaffected (no fee), `deposit`/`stake` work, so users can enter but
not instantly exit until the admin calls `set_config`.

## Impact

Instant-exit DoS on a misconfigured deployment; no state corruption (the panic is before any
await in the vault and the pool is synchronous). Recoverable by the admin via `set_config`.
Deployer-controlled only, hence Low.

## Suggested fix

Validate in the constructor exactly as `set_config` does (reject `instant_fee_bps > MAX_FEE_BPS`
and out-of-range vesting; consider bounding `unbond_period_secs` too), or route the constructor
through `set_config`. Independently, replace `assets - fee` with
`assets.checked_sub(fee).ok_or(VaultError::Overflow)?` (or `saturating_sub`) so the invariant
`fee <= assets` is not load-bearing for a trap.

> Overlap: same root cause as `arith-1.md` (filed independently by the arithmetic worker); merge at triage.
