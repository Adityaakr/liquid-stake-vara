# Dimensional Units — Vale Protocol (kVARA pool, kUSDC/kUSDT vaults, app, deploy script)

Notation follows the Trail of Bits dimensional-analysis skill: `D<n>{unit}` is a fixed-point value
with `n` implied decimals; `{1}` is dimensionless; `{A/B}` is a derived unit. Scope of this vocabulary:

- `programs/vara-pool/app/src/lib.rs` (on-chain kVARA pool)
- `programs/vault/app/src/lib.rs` (on-chain kUSDC / kUSDT vaults)
- `src/domain/math.ts`, `src/chain/varaPool.ts`, `src/chain/gearAdapter.ts` (app)
- `scripts/deploy.ts` (deployment: tranche sizing, start rate)

## Base Units

| Unit | Meaning | Precision | Where it lives |
|------|---------|-----------|----------------|
| `{VARA}` | Native VARA | **D12** (planck) | `Syscall::message_value()`, `reserve`, `fees_accrued`, `unbonding_total`, `vest_amount`, `distributable`, `MIN_STAKE`, `Unbond.assets`, `UnstakePreview.*`, `balances.VARA`, `ONE_VARA`, `STAKED_VARA`, `SESSION_MIN_GAS`, deploy `*_VARA * ONE_VARA`, `REWARDS_VARA` |
| `{asset}` (= `{USDC}` or `{USDT}`) | Demo stable underlying of one vault | **D6** | vault `holdings`, `fees_accrued`, `unbonding_total`, `vest_amount`, `distributable`, `MIN_DEPOSIT`, `RedeemPreview.*`, `Unbond.assets`, `balances.USDC/USDT`, `ONE_STABLE`, deploy `ONE`, `FAUCET_AMOUNT`, `SEED_TOKENS * ONE` |
| `{kVARA}` (a `{share}`) | Receipt share of the kVARA pool | **D12** (pool `decimals = 12`) | pool `total_shares`, `balances`, `allowances`, `Unbond.shares`, `Position.shares`, `balances.kVARA` |
| `{kUSDC}` / `{kUSDT}` (a `{share}`) | Receipt share of a vault | **D6** (vault `decimals = 6`) | vault `total_shares`, `balances`, `allowances`, `Unbond.shares`, `balances.kUSDC/kUSDT` |
| `{ms}` | Milliseconds since Unix epoch, or a duration in ms | integer | `Syscall::block_timestamp()` (Gear reports **ms**), `Date.now()`, `vest_start_ms`, `vest_end_ms`, `unbond_period_ms`, `vesting_period_ms`, `claimable_at`, `requested_at`, `expires_at`, `vesting_ends_at`, `T0`, `ERA_MS`, `YEAR_MS`, `elapsedMs`, `stats.at`, `eraEndsAt`, `vestingEndsAt`, `POLL_MS` |
| `{s}` | Seconds (durations only) | integer | `unbond_period_secs`, `vesting_period_secs`, `duration_secs`, `YEAR_SECS`, `MIN/MAX_VESTING_SECS`, `MAX_SESSION_SECS`, `BLOCK_SECS`, `UNBOND_SECS`, `VESTING_SECS`, `FAUCET_COOLDOWN_SECS`, `durationSecs`, `unbondSecs`, `cooldownSecs` |
| `{1}` | Dimensionless ratio / count | — | fee and APY basis points (see D4), `decimals`, loop indices, `n` blocks |
| `{USD}` | US-dollar value | IEEE double | `tvlUsd`, `VARA_TVL_USD`, `STABLE_TVL_USD` |
| `{block}` / `{era}` | Chain block number / simulated 12-hour era index | integer | `blockNumber`, `era`, `BASE_ERA`, `rateHistory[].era` |
| `{gas}` | Gear gas units | integer | `TOKEN_CALL_GAS`, `GAS_ONE_CALL`, `GAS_INIT`, `GAS_CALL`, `GAS.*` (no dimensional arithmetic; listed for completeness) |

Notes

- Shares are minted 1:1 with assets at rate 1.0 and carry the **same decimals as their asset**
  (`12` for kVARA, `6` for kUSDC/kUSDT). Because of that, the `asset/share` rate is decimals-neutral:
  `D12{VARA} * D18 / D12{kVARA}` and `D6{asset} * D18 / D6{share}` both yield a plain `D18{asset/share}`.
- `block_timestamp` on Gear is milliseconds; every on-chain period is converted with `* 1000` at the
  config boundary and stored as `*_ms`. Seconds only ever appear in configuration inputs and outputs.

## Derived Units

| Unit | Meaning | Precision | Where it lives |
|------|---------|-----------|----------------|
| `{VARA/kVARA}`, `{asset/share}` | Exchange rate: assets owed per receipt share | **D18** on chain (`SCALE = 1e18`) | `base_rate`, `initial_rate`, `rate_at()`, `PoolInfo.rate`, `VaultInfo.rate`, `Staked/Unstaked/Deposited/Redeemed.rate`, deploy `startRate()` output |
| `{VARA/kVARA}`, `{asset/share}` | Same rate in the app | **D9** (`RATE_SCALE = 1e9`) | `BASE_RATE`, `STABLE_BASE_PRICE`, `varaRateAt()`, `stablePriceAt()`, `stats.rate`, `vaults[a].rate`, `rateHistory[].rate`, `card.rate`, every `rate`/`sharePrice` argument in `math.ts`, deploy `startRate()` input |
| `{1/yr}` | Annualised yield (per 365-day year) | **D4** (basis points) | `apy_bps()`, `PoolInfo.apy_bps`, `VaultInfo.apy_bps`, `apyBps`, `VARA_APY_BPS`, `STABLE_APY_BPS`, `POOL_APY_BPS`, `ASSETS[].apyBps` |
| `{1}` fee fraction | Instant-exit fee | **D4** (bps) | `instant_fee_bps`, `MAX_FEE_BPS`, `INSTANT_UNSTAKE_FEE_BPS`, `INSTANT_FEE_BPS`, `feeBps`, `stakeFeeBps`, `instantFeeBps` |
| `{1}` reserve ratio | `reserve / total_assets` | **D4** (bps) | `bufferBps` (gear adapter; note the mock uses a different definition — see DIM-002) |
| `{asset/ms}` | Linear reward release rate of the vesting tranche | implicit | `vest_amount / (vest_end_ms - vest_start_ms)` inside `locked_rewards()` and `apy_bps()` |
| `{USD/VARA}`, `{USD/asset}` | Prices | IEEE double | `VARA_PRICE_USD = 0.0004258`, `STABLE_PRICE_USD = 1` |
| `{ms/s}` | Time scale factor | 1000 | every `secs.saturating_mul(1000)`, `/ 1000`, `BLOCK_SECS * 1000`, `durationSecs * 1000` |
| `{s/yr}` | Year length | `YEAR_SECS = 31_536_000` | programs, deploy, gear adapter literal `31_536_000n` |
| `{ms/yr}` | Year length in ms | `YEAR_SECS * 1000 = 31_536_000_000`, `YEAR_MS = 31_536_000_000n` | `apy_bps()`, `accrueRate()` |
| `{ms/era}` | Simulated era length | `ERA_MS = 43_200_000` | `varaEra()`, `varaRateHistory()` |
| `{s/block}` | Block time | `BLOCK_SECS = 3` | `rateBlocksAgo()`, `eraEndsAt` |
| `{day/yr}` | | `365n` | `projectedYield()` |

## Precision Prefixes

| Prefix | Value | Used for |
|--------|-------|----------|
| `D3` | 1e3 | ms per second (`* 1000`, `/ 1000`) |
| `D4` | 1e4 | basis points: fees, APY, buffer ratio (`BPS = 10_000`) |
| `D6` | 1e6 | demo USDC / USDT base units and their receipt shares (`ONE_STABLE`, deploy `ONE`) |
| `D9` | 1e9 | app-side exchange rate (`RATE_SCALE`); also the D18→D9 / D9→D18 scale factor (`RATE_1E18_TO_1E9`, `RATE_SCALE_UP`) |
| `D12` | 1e12 | VARA planck and kVARA shares (`ONE_VARA`, `VARA_DECIMALS`) |
| `D18` | 1e18 | on-chain exchange rate (`SCALE`) |

Scale-crossing points (the only places precision changes)

| Direction | Expression | File |
|-----------|------------|------|
| D9 → D18 | `rate1e9 * RATE_SCALE_UP` | `scripts/deploy.ts:62` |
| D18 → D9 | `big(info.rate) / RATE_1E18_TO_1E9` | `src/chain/gearAdapter.ts:347,367` (truncates 9 decimals; display/preview only) |
| whole → D12 | `x * ONE_VARA`, `Math.round(gasVara * 1e12)` | `scripts/deploy.ts`, `src/chain/gearAdapter.ts:272` |
| D12 → whole (float) | `Number(x) / 1e12`, `Number(x) / Number(ONE_VARA)` | display / TVL only |
| whole → D6 | `x * ONE` | `scripts/deploy.ts` |
| D6 → whole (float) | `Number(x) / 1e6`, `Number(x) / Number(ONE_STABLE)` | display / TVL only |
| s → ms | `secs * 1000` | program constructors, `set_config`, `propose_session`, adapter session bookkeeping |
| ms → s | `*_ms / 1000` | `info()` only |
