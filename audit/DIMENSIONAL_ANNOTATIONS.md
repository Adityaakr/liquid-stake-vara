# Dimensional Annotations — every arithmetic expression in scope

Format: `file:line  expression  => dimension  [confidence]`.
Confidence: **CERTAIN** (dimension fixed by a constant, type doc, or an in-scope definition),
**INFERRED** (derived through propagation or naming), **UNCERTAIN** (depends on out-of-scope code).
Vocabulary: see `DIMENSIONAL_UNITS.md`. Paths are relative to `/Users/adityakrx/liquid-stake`.

File legend: `pool` = `programs/vara-pool/app/src/lib.rs`, `vault` = `programs/vault/app/src/lib.rs`,
`math` = `src/domain/math.ts`, `vp` = `src/chain/varaPool.ts`, `ga` = `src/chain/gearAdapter.ts`,
`deploy` = `scripts/deploy.ts`.

---

## programs/vara-pool/app/src/lib.rs  (kVARA pool; asset = D12{VARA}, share = D12{kVARA})

### Anchors (constants, struct fields)

```
pool:26   SCALE = 1e18                                  => D18 scale constant {1}                       [CERTAIN]
pool:27   BPS = 10_000                                  => D4 scale constant {1}                        [CERTAIN]
pool:28   YEAR_SECS = 31_536_000                        => {s/yr}                                       [CERTAIN]
pool:30   MIN_STAKE = 10_000_000_000                    => D12{VARA} (0.01 VARA)                        [CERTAIN]
pool:31   MAX_FEE_BPS = 1_000                           => D4{1}                                        [CERTAIN]
pool:33   MIN_VESTING_SECS = 3_600                      => {s}                                          [CERTAIN]
pool:34   MAX_VESTING_SECS = 365 * 86_400               => {day}*{s/day} = {s}                          [CERTAIN]
pool:36   MAX_SESSION_SECS = 30 * 86_400                => {s}                                          [CERTAIN]
pool:87   Session.expires_at                            => {ms}                                         [CERTAIN]
pool:95   Unbond.shares                                 => D12{kVARA}                                   [CERTAIN]
pool:96   Unbond.assets                                 => D12{VARA}                                    [CERTAIN]
pool:97   Unbond.requested_at / :98 claimable_at        => {ms}                                         [CERTAIN]
pool:104  UnstakePreview.assets / fee / net             => D12{VARA}                                    [CERTAIN]
pool:112  Position.shares / :113 assets                 => D12{kVARA} / D12{VARA}                       [CERTAIN]
pool:124  PoolInfo.rate                                 => D18{VARA/kVARA}                              [CERTAIN]
pool:127  PoolInfo.apy_bps                              => D4{1/yr}                                     [CERTAIN]
pool:128  PoolInfo.instant_fee_bps                      => D4{1}                                        [CERTAIN]
pool:129  PoolInfo.unbond_period_secs / :130 vesting    => {s}                                          [CERTAIN]
pool:131  PoolInfo.total_shares                         => D12{kVARA}                                   [CERTAIN]
pool:133  total_assets, :135 reserve, :137 distributable, :139 locked_rewards, :142 fees_accrued,
          :143 unbonding_total, :144 min_stake          => D12{VARA}                                    [CERTAIN]
pool:141  PoolInfo.vesting_ends_at                      => {ms}                                         [CERTAIN]
pool:157  PoolState.total_shares                        => D12{kVARA}                                   [CERTAIN]
pool:158  balances / :159 allowances (values)           => D12{kVARA}                                   [CERTAIN]
pool:162  base_rate                                     => D18{VARA/kVARA}                              [CERTAIN]
pool:163  instant_fee_bps                               => D4{1}                                        [CERTAIN]
pool:164  unbond_period_ms / :165 vesting_period_ms     => {ms}                                         [CERTAIN]
pool:168  reserve, :169 fees_accrued, :170 unbonding_total, :173 vest_amount  => D12{VARA}              [CERTAIN]
pool:174  vest_start_ms / :175 vest_end_ms              => {ms}                                         [CERTAIN]
```

### Arithmetic

```
pool:198  if initial_rate < SCALE { SCALE } else { initial_rate }
          => D18{VARA/kVARA} vs D18 constant; result D18{VARA/kVARA}                                    [CERTAIN]
pool:200  unbond_period_secs.saturating_mul(1000)       => {s} * {ms/s} = {ms}                          [CERTAIN]
pool:201  vesting_period_secs.clamp(MIN_VESTING_SECS, MAX_VESTING_SECS).saturating_mul(1000)
          => {s} clamped to {s} bounds, * {ms/s} = {ms}                                                 [CERTAIN]
pool:221  now_ms >= self.vest_end_ms                    => {ms} vs {ms}                                 [CERTAIN]
pool:222  now_ms <= self.vest_start_ms                  => {ms} vs {ms}                                 [CERTAIN]
pool:223  span = vest_end_ms - vest_start_ms            => {ms} - {ms} = {ms}                           [CERTAIN]
pool:224  left = vest_end_ms - now_ms                   => {ms} - {ms} = {ms}                           [CERTAIN]
pool:225  vest_amount.saturating_mul(left) / span       => D12{VARA} * {ms} / {ms} = D12{VARA}          [CERTAIN]
          (implicit release rate vest_amount/span is {VARA/ms}; multiply-before-divide, rounds down)
pool:230  reserve - fees_accrued - unbonding_total - locked_rewards(now)
          => D12{VARA} - D12{VARA} - D12{VARA} - D12{VARA} = D12{VARA}                                  [CERTAIN]
pool:235  total_shares.is_zero() || distributable.is_zero()   => predicate on D12{kVARA}, D12{VARA}     [CERTAIN]
pool:241  distributable(now) * SCALE / total_shares
          => D12{VARA} * D18 / D12{kVARA} = D18{VARA/kVARA}   (D12 decimals cancel)                      [CERTAIN]
pool:247  assets * SCALE / base_rate
          => D12{VARA} * D18 / D18{VARA/kVARA} = D12{kVARA}                                             [CERTAIN]
pool:249  assets * total_shares / distributable(now)
          => D12{VARA} * D12{kVARA} / D12{VARA} = D12{kVARA}                                            [CERTAIN]
pool:255  shares * base_rate / SCALE
          => D12{kVARA} * D18{VARA/kVARA} / D18 = D12{VARA}                                             [CERTAIN]
pool:257  shares * distributable(now) / total_shares
          => D12{kVARA} * D12{VARA} / D12{kVARA} = D12{VARA}                                            [CERTAIN]
pool:261  total_assets = distributable(now) (or 0)      => D12{VARA}                                    [CERTAIN]
pool:268  span = vest_end_ms - vest_start_ms            => {ms}                                         [CERTAIN]
pool:269  vest_amount * (YEAR_SECS * 1000) * BPS / (span * d)
          => D12{VARA} * {ms/yr} * D4 / ({ms} * D12{VARA}) = D4{1/yr}   (all units cancel)              [CERTAIN]
          YEAR_SECS * 1000 = 31_536_000_000 (u64, no overflow); U256 product ~1e36 max, no overflow
pool:270  bps > u32::MAX ? u32::MAX : bps.low_u32()     => D4{1/yr} (saturating narrow)                 [CERTAIN]
pool:274  assets = assets_for(shares, now)              => D12{VARA}                                    [CERTAIN]
pool:275  assets * instant_fee_bps / BPS                => D12{VARA} * D4{1} / D4 = D12{VARA}           [CERTAIN]
pool:276  net: assets - fee                             => D12{VARA} - D12{VARA} = D12{VARA}            [CERTAIN]
pool:294  balance_of(to) + value                        => D12{kVARA} + D12{kVARA}                      [CERTAIN]
pool:300  balance_of(from) - value                      => D12{kVARA} - D12{kVARA}                      [CERTAIN]
pool:323  current - value  (allowance)                  => D12{kVARA} - D12{kVARA}                      [CERTAIN]
pool:329  total_shares + value                          => D12{kVARA}                                   [CERTAIN]
pool:340  total_shares - value                          => D12{kVARA}                                   [CERTAIN]
pool:341  base_rate = rate_before                       => D18{VARA/kVARA} := D18{VARA/kVARA}           [CERTAIN]
pool:352  fees_accrued + orphaned                       => D12{VARA} + D12{VARA}                        [CERTAIN]
pool:360  assets < U256::from(MIN_STAKE)                => D12{VARA} vs D12{VARA}                       [CERTAIN]
pool:362  shares = shares_for(assets, now)              => D12{kVARA}                                   [CERTAIN]
pool:365  reserve + assets                              => D12{VARA} + D12{VARA}                        [CERTAIN]
pool:374  balance_of(owner) < shares                    => D12{kVARA} vs D12{kVARA}                     [CERTAIN]
pool:378  p.assets > available                          => D12{VARA} vs D12{VARA}                       [CERTAIN]
pool:380  fees_accrued + p.fee                          => D12{VARA}                                    [CERTAIN]
pool:381  reserve -= p.net                              => D12{VARA} - D12{VARA}                        [CERTAIN]
pool:388  fees_accrued - p.fee / :389 reserve + p.net   => D12{VARA}                                    [CERTAIN]
pool:398  remaining = locked_rewards(now)               => D12{VARA}                                    [CERTAIN]
pool:399  period = vesting_period_ms                    => {ms}                                         [CERTAIN]
pool:400  total = remaining + assets                    => D12{VARA} + D12{VARA} = D12{VARA}            [CERTAIN]
pool:404  left = vest_end_ms - now_ms                   => {ms}                                         [CERTAIN]
pool:405  (remaining * left + assets * period) / total
          => (D12{VARA}*{ms} + D12{VARA}*{ms}) / D12{VARA} = {ms}   (amount-weighted mean of two durations) [CERTAIN]
pool:407  time_left.max(1).low_u64()                    => {ms}                                         [CERTAIN]
pool:410  vest_end_ms = now_ms + time_left              => {ms} + {ms} = {ms}                           [CERTAIN]
pool:411  reserve + assets                              => D12{VARA}                                    [CERTAIN]
pool:419  assets = assets_for(shares, now)              => D12{VARA}                                    [CERTAIN]
pool:422  assets > available                            => D12{VARA} vs D12{VARA}                       [CERTAIN]
pool:424  unbonding_total + assets                      => D12{VARA}                                    [CERTAIN]
pool:425  claimable_at: now_ms + unbond_period_ms       => {ms} + {ms} = {ms}                           [CERTAIN]
pool:426  next_unbond_id += 1                           => {1} counter                                  [CERTAIN]
pool:436  now_ms < claimable_at                         => {ms} vs {ms}                                 [CERTAIN]
pool:437  assets > reserve                              => D12{VARA} vs D12{VARA}                       [CERTAIN]
pool:441  unbonding_total - entry.assets / :442 reserve -= entry.assets   => D12{VARA}                  [CERTAIN]
pool:448  unbonding_total + assets / :449 reserve + assets                => D12{VARA}                  [CERTAIN]
pool:464  instant_fee_bps > MAX_FEE_BPS                 => D4{1} vs D4{1}                               [CERTAIN]
pool:464  (MIN_VESTING_SECS..=MAX_VESTING_SECS).contains(&vesting_period_secs)  => {s} range vs {s}     [CERTAIN]
pool:466  unbond_period_secs * 1000                     => {s} * {ms/s} = {ms}                          [CERTAIN]
pool:467  vesting_period_secs * 1000                    => {ms}                                         [CERTAIN]
pool:477  now_ms >= session.expires_at                  => {ms} vs {ms}                                 [CERTAIN]
pool:484  duration_secs > MAX_SESSION_SECS              => {s} vs {s}                                   [CERTAIN]
pool:487  expires_at: now_ms + duration_secs * 1000     => {ms} + {s}*{ms/s} = {ms}                     [CERTAIN]
pool:496  now_ms >= proposed.expires_at                 => {ms} vs {ms}                                 [CERTAIN]
pool:519  rate: rate_at(now)                            => D18{VARA/kVARA}                              [CERTAIN]
pool:520  apy_bps: apy_bps(now)                         => D4{1/yr}                                     [CERTAIN]
pool:522  unbond_period_ms / 1000                       => {ms} / {ms/s} = {s}  (exact: stored as secs*1000) [CERTAIN]
pool:523  vesting_period_ms / 1000                      => {s}                                          [CERTAIN]
pool:529  vesting_ends_at: vest_end_ms (or 0)           => {ms}                                         [CERTAIN]
pool:532  min_stake: MIN_STAKE                          => D12{VARA}                                    [CERTAIN]
pool:662  now_ms() = Syscall::block_timestamp()         => {ms}  (Gear block timestamp is ms)           [CERTAIN]
pool:667  as_value(v): U256 -> u128                     => D12{VARA} (VARA supply << u128::MAX)         [CERTAIN]
pool:673  send_bytes_with_gas(to, [], 0, as_value(assets))   => gas {gas}=0, value D12{VARA}            [CERTAIN]
pool:682  value = message_value() / :683 assets = U256::from(value)   => D12{VARA}                      [CERTAIN]
pool:687  s.stake(owner, assets, now) -> shares; rate_at(now)   => D12{kVARA}; D18{VARA/kVARA}          [CERTAIN]
pool:762  value = message_value() / :764 assets        => D12{VARA} (rewards)                           [CERTAIN]
pool:839  amount > reserve / :841 reserve -= amount     => D12{VARA}                                    [CERTAIN]
pool:852  reserve + amount                              => D12{VARA}                                    [CERTAIN]
pool:892  now_ms() < x.expires_at (:898, :905 same)     => {ms} vs {ms}                                 [CERTAIN]
pool:956  Program::new(..., initial_rate)               => param D18{VARA/kVARA} (doc: "1e18 scale, 0 = 1.0") [CERTAIN]
```

Test-module arithmetic (`pool:975+`) uses `ONE = 1e12` D12{VARA}, `DAY = 86_400 * 1000` {ms}, `WEEK_SECS` {s};
`SCALE + SCALE / 100` etc. are D18{VARA/kVARA}; `apy_bps(T0) == 365 * 100` checks 7 VARA / 7 days over 100 VARA
= 1 %/day = 36 500 bps. All consistent with the vocabulary; not audited further (test code).

---

## programs/vault/app/src/lib.rs  (kUSDC / kUSDT vault; asset = D6{asset}, share = D6{share})

Structurally identical to the pool with `holdings` in place of `reserve`. Line-by-line:

```
vault:34   SCALE = 1e18                                 => D18 scale constant                           [CERTAIN]
vault:35   BPS = 10_000                                 => D4 scale constant                            [CERTAIN]
vault:36   YEAR_SECS = 31_536_000                       => {s/yr}                                       [CERTAIN]
vault:38   MIN_DEPOSIT = 1_000                          => D6{asset} (0.001 USDC/USDT)                  [INFERRED: assumes a 6-decimal underlying; see DIM-003]
vault:39   MAX_FEE_BPS = 1_000                          => D4{1}                                        [CERTAIN]
vault:41   MIN_VESTING_SECS / :42 MAX_VESTING_SECS / :44 MAX_SESSION_SECS   => {s}                      [CERTAIN]
vault:46   TOKEN_CALL_GAS / :50 GAS_ONE_CALL            => {gas}                                        [CERTAIN]
vault:105  Session.expires_at                           => {ms}                                         [CERTAIN]
vault:113  Unbond.shares / :114 assets / :115-116 *_at  => D6{share} / D6{asset} / {ms}                 [CERTAIN]
vault:122  RedeemPreview.assets/fee/net                 => D6{asset}                                    [CERTAIN]
vault:143  VaultInfo.rate                               => D18{asset/share}                             [CERTAIN]
vault:146  VaultInfo.apy_bps                            => D4{1/yr}                                     [CERTAIN]
vault:147  instant_fee_bps                              => D4{1}                                        [CERTAIN]
vault:148-149 *_period_secs                             => {s}                                          [CERTAIN]
vault:150  total_shares                                 => D6{share}                                    [CERTAIN]
vault:152,154,156,158,161,162,163  asset totals         => D6{asset}                                    [CERTAIN]
vault:160  vesting_ends_at                              => {ms}                                         [CERTAIN]
vault:177-179 total_shares, balances, allowances        => D6{share}                                    [CERTAIN]
vault:182  base_rate                                    => D18{asset/share}                             [CERTAIN]
vault:184-185 *_period_ms                               => {ms}                                         [CERTAIN]
vault:188-190,193 holdings, fees_accrued, unbonding_total, vest_amount   => D6{asset}                   [CERTAIN]
vault:194-195 vest_start_ms / vest_end_ms               => {ms}                                         [CERTAIN]

vault:219  if initial_rate < SCALE { SCALE } else { initial_rate }   => D18{asset/share}                [CERTAIN]
vault:221  unbond_period_secs * 1000                    => {ms}                                         [CERTAIN]
vault:222  vesting_period_secs.clamp(..) * 1000         => {ms}                                         [CERTAIN]
vault:242-243 now_ms vs vest_end_ms / vest_start_ms     => {ms} vs {ms}                                 [CERTAIN]
vault:244  span = vest_end_ms - vest_start_ms           => {ms}                                         [CERTAIN]
vault:245  left = vest_end_ms - now_ms                  => {ms}                                         [CERTAIN]
vault:246  vest_amount * left / span                    => D6{asset} * {ms} / {ms} = D6{asset}          [CERTAIN]
vault:251  holdings - fees_accrued - unbonding_total - locked_rewards   => D6{asset}                    [CERTAIN]
vault:262  distributable * SCALE / total_shares         => D6{asset} * D18 / D6{share} = D18{asset/share} [CERTAIN]
vault:268  assets * SCALE / base_rate                   => D6{asset} * D18 / D18{asset/share} = D6{share} [CERTAIN]
vault:270  assets * total_shares / distributable        => D6{asset} * D6{share} / D6{asset} = D6{share} [CERTAIN]
vault:276  shares * base_rate / SCALE                   => D6{share} * D18{asset/share} / D18 = D6{asset} [CERTAIN]
vault:278  shares * distributable / total_shares        => D6{asset}                                    [CERTAIN]
vault:282  total_assets                                 => D6{asset}                                    [CERTAIN]
vault:289  span                                         => {ms}                                         [CERTAIN]
vault:290  vest_amount * (YEAR_SECS * 1000) * BPS / (span * d)
           => D6{asset} * {ms/yr} * D4 / ({ms} * D6{asset}) = D4{1/yr}                                  [CERTAIN]
vault:291  saturate to u32                              => D4{1/yr}                                     [CERTAIN]
vault:295  assets_for(shares)                           => D6{asset}                                    [CERTAIN]
vault:296  assets * instant_fee_bps / BPS               => D6{asset} * D4{1} / D4 = D6{asset}           [CERTAIN]
vault:297  assets - fee                                 => D6{asset}                                    [CERTAIN]
vault:315  balance + value / :321 balance - value       => D6{share}                                    [CERTAIN]
vault:344  current - value (allowance)                  => D6{share}                                    [CERTAIN]
vault:350  total_shares + value / :361 total_shares - value   => D6{share}                              [CERTAIN]
vault:362  base_rate = rate_before                      => D18{asset/share}                             [CERTAIN]
vault:373  fees_accrued + orphaned                      => D6{asset}                                    [CERTAIN]
vault:381  assets < MIN_DEPOSIT                         => D6{asset} vs D6{asset}                       [CERTAIN]
vault:382  shares_for(assets)                           => D6{share}                                    [CERTAIN]
vault:390  shares_for(assets)?.max(U256::one())         => D6{share} (floor of 1 base share = 1e-6 kUSDC) [CERTAIN]
vault:392  holdings + assets                            => D6{asset}                                    [CERTAIN]
vault:401  balance_of(owner) < shares                   => D6{share} vs D6{share}                       [CERTAIN]
vault:405  p.assets > available                         => D6{asset} vs D6{asset}                       [CERTAIN]
vault:407  fees_accrued + p.fee / :408 holdings -= p.net       => D6{asset}                             [CERTAIN]
vault:415  fees_accrued - p.fee / :416 holdings + p.net        => D6{asset}                             [CERTAIN]
vault:425  remaining = locked_rewards(now)              => D6{asset}                                    [CERTAIN]
vault:426  period = vesting_period_ms                   => {ms}                                         [CERTAIN]
vault:427  total = remaining + assets                   => D6{asset}                                    [CERTAIN]
vault:431  left = vest_end_ms - now_ms                  => {ms}                                         [CERTAIN]
vault:432  (remaining * left + assets * period) / total => (D6{asset}*{ms} + D6{asset}*{ms}) / D6{asset} = {ms} [CERTAIN]
vault:434  time_left.max(1).low_u64()                   => {ms}                                         [CERTAIN]
vault:437  vest_end_ms = now_ms + time_left             => {ms}                                         [CERTAIN]
vault:438  holdings + assets                            => D6{asset}                                    [CERTAIN]
vault:446  assets_for(shares)                           => D6{asset}                                    [CERTAIN]
vault:449  assets > available                           => D6{asset} vs D6{asset}                       [CERTAIN]
vault:451  unbonding_total + assets                     => D6{asset}                                    [CERTAIN]
vault:452  claimable_at: now_ms + unbond_period_ms      => {ms}                                         [CERTAIN]
vault:453  next_unbond_id += 1                          => {1}                                          [CERTAIN]
vault:463  now_ms < claimable_at                        => {ms} vs {ms}                                 [CERTAIN]
vault:464  assets > holdings                            => D6{asset} vs D6{asset}                       [CERTAIN]
vault:468-469, 475-476  unbonding_total / holdings ± entry.assets   => D6{asset}                        [CERTAIN]
vault:486  amount > holdings / :488 holdings -= amount  => D6{asset}                                    [CERTAIN]
vault:493-494 fees_accrued / holdings + amount          => D6{asset}                                    [CERTAIN]
vault:506  fee & vesting range checks                   => D4{1}; {s} vs {s}                            [CERTAIN]
vault:508-509 *_secs * 1000                             => {ms}                                         [CERTAIN]
vault:519  now_ms >= expires_at                         => {ms}                                         [CERTAIN]
vault:526  duration_secs > MAX_SESSION_SECS             => {s} vs {s}                                   [CERTAIN]
vault:529  now_ms + duration_secs * 1000                => {ms}                                         [CERTAIN]
vault:538  now_ms >= proposed.expires_at                => {ms}                                         [CERTAIN]
vault:562  rate_at(now) / :563 apy_bps(now)             => D18{asset/share} / D4{1/yr}                  [CERTAIN]
vault:565-566 *_period_ms / 1000                        => {s}                                          [CERTAIN]
vault:572  vesting_ends_at: vest_end_ms (or 0)          => {ms}                                         [CERTAIN]
vault:575  min_deposit: MIN_DEPOSIT                     => D6{asset}                                    [CERTAIN]
vault:705  now_ms() = block_timestamp()                 => {ms}                                         [CERTAIN]
vault:710  gas_available() < required                   => {gas} vs {gas}                               [CERTAIN]
vault:716  transfer_from(from, me, assets)              => arg D6{asset} (VFT amount)                   [CERTAIN]
vault:726  transfer(to, assets)                         => arg D6{asset}                                [CERTAIN]
vault:744  check_deposit(assets) / :751 settle_deposit(assets) -> shares   => D6{asset} -> D6{share}    [CERTAIN]
vault:1009 Program::new(..., initial_rate)              => param D18{asset/share}                       [CERTAIN]
```

Test-module arithmetic (`vault:1030+`): `ONE = 1_000_000` D6, `DAY` {ms}, `apy_bps(T0) == 36_500` — consistent.

---

## src/domain/math.ts  (app; rate is D9{asset/share})

```
math:12   (vara * RATE_SCALE) / rate
          => D12{VARA} * D9 / D9{VARA/kVARA} = D12{kVARA}                                               [CERTAIN]
math:17   (kvara * rate) / RATE_SCALE
          => D12{kVARA} * D9{VARA/kVARA} / D9 = D12{VARA}                                               [CERTAIN]
math:21   (varaOut * INSTANT_UNSTAKE_FEE_BPS) / BPS
          => D12{VARA} * D4{1} / D4 = D12{VARA}    (fixed 30 bps; UI uses the chain's fee instead)      [CERTAIN]
math:26   gross = kVaraToVara(kvara, rate)              => D12{VARA}                                    [CERTAIN]
math:27   fee = instantUnstakeFee(gross)                => D12{VARA}                                    [CERTAIN]
math:28   net: gross - fee                              => D12{VARA}                                    [CERTAIN]
math:33   kVaraToVara(kvara, rate)                      => D12{VARA}                                    [CERTAIN]
math:39   (assets * RATE_SCALE) / sharePrice
          => D6{asset} * D9 / D9{asset/share} = D6{share}   (also used with D12{VARA} → D12{kVARA})     [CERTAIN]
math:43   (shares * sharePrice) / RATE_SCALE
          => D6{share} * D9{asset/share} / D9 = D6{asset}                                               [CERTAIN]
math:48   (principal * apyBps * BigInt(Math.round(days))) / (BPS * 365n)
          => {asset} * D4{1/yr} * {day} / (D4 * {day/yr}) = {asset}                                     [CERTAIN]
math:51   YEAR_MS = 365n * 24n * 60n * 60n * 1000n      => {day/yr}*{h/day}*{min/h}*{s/min}*{ms/s} = {ms/yr} = 31_536_000_000 [CERTAIN]
math:58   elapsedMs <= 0                                => {ms} guard                                   [CERTAIN]
math:59   rate + (rate * apyBps * BigInt(Math.floor(elapsedMs))) / (BPS * YEAR_MS)
          => D9{a/s} + D9{a/s} * D4{1/yr} * {ms} / (D4 * {ms/yr}) = D9{a/s}                             [CERTAIN]
          (simple interest; matches the chain's linear release exactly since rate*apy is constant over a tranche)
math:67   Math.min(now, vestingEndsAt) - at             => min({ms},{ms}) - {ms} = {ms}                 [CERTAIN]
          vestingEndsAt = 0 (nothing vesting) => negative => rate unchanged; Infinity => now - at
```

---

## src/chain/varaPool.ts  (published figures; D9 rate)

```
vp:11    T0 = Date.UTC(2026, 8, 1)                     => {ms}                                          [CERTAIN]
vp:12    BASE_ERA = 4182                                => {era}                                        [CERTAIN]
vp:13    ERA_MS = 12 * 60 * 60 * 1000                   => {h}*{min/h}*{s/min}*{ms/s} = {ms/era} = 43_200_000 [CERTAIN]
vp:14    BASE_RATE = parseRate('1.0482')                => D9{VARA/kVARA} (parseUnits(s, 9))            [CERTAIN]
vp:16    VARA_APY_BPS = 3500n                           => D4{1/yr}                                     [CERTAIN]
vp:18    STAKED_VARA = 918_270_000n * ONE_VARA          => {VARA} * D12 = D12{VARA}                     [CERTAIN]
vp:19    VARA_PRICE_USD = 0.0004258                     => {USD/VARA} (float)                           [CERTAIN]
vp:23    Math.max(0, Math.floor((now - T0) / 1000) * 1000)
         => ({ms} - {ms}) / {ms/s} * {ms/s} = {ms}, floored to whole seconds                            [CERTAIN]
vp:27    accrueRate(BASE_RATE, VARA_APY_BPS, elapsedMs(now))   => D9{VARA/kVARA}                        [CERTAIN]
vp:32    BASE_ERA + Math.floor(elapsed / ERA_MS)        => {era} + {ms} / {ms/era} = {era}              [CERTAIN]
vp:33    T0 + (era - BASE_ERA + 1) * ERA_MS             => {ms} + {era} * {ms/era} = {ms}               [CERTAIN]
vp:39    Math.max(0, (e - BASE_ERA) * ERA_MS)           => {era} * {ms/era} = {ms}                      [CERTAIN]
vp:39    accrueRate(BASE_RATE, VARA_APY_BPS, ms)        => D9{VARA/kVARA}                               [CERTAIN]
vp:43    (Number(STAKED_VARA) / Number(ONE_VARA)) * VARA_PRICE_USD
         => D12{VARA} / D12 * {USD/VARA} = {USD} (float; 918.27M * 0.0004258 = 391 019)                 [CERTAIN]
vp:46    STABLE_APY_BPS = { USDT: 840n, USDC: 790n }    => D4{1/yr}                                     [CERTAIN]
vp:47    STABLE_BASE_PRICE = parseRate('1.18' / '1.15') => D9{asset/share}                              [CERTAIN]
vp:48    STABLE_TVL_USD                                 => {USD}                                        [CERTAIN]
vp:51    accrueRate(STABLE_BASE_PRICE[a], STABLE_APY_BPS[a], elapsedMs(now))   => D9{asset/share}       [CERTAIN]
```

---

## src/chain/gearAdapter.ts  (chain reads and conversions)

```
ga:23    RATE_1E18_TO_1E9 = 1_000_000_000n              => D9 scale factor {1}                          [CERTAIN]
ga:25    STABLE_PRICE_USD = 1                           => {USD/asset}                                  [CERTAIN]
ga:26    POLL_MS = 12_000                               => {ms}                                         [CERTAIN]
ga:27    BLOCK_SECS = 3                                 => {s/block}                                    [CERTAIN]
ga:54    SESSION_MIN_GAS = 11n * 10n ** 12n             => {VARA} * D12 = D12{VARA}                     [CERTAIN]
ga:56    SESSION_DEFAULT_GAS_VARA = 15                  => {VARA} (whole units, scaled at ga:272)        [CERTAIN]
ga:152   api.balance.transfer(address, 100_000_000_000_000)   => D12{VARA} (100 VARA)                   [CERTAIN]
ga:217   stored.expiresAt <= Date.now() + 60_000        => {ms} vs {ms} + {ms}                          [CERTAIN]
ga:222   gas = balance.findOut(pair.address)            => D12{VARA} (named "gas", is VARA)             [CERTAIN]
ga:224   gas < SESSION_MIN_GAS                          => D12{VARA} vs D12{VARA}                       [CERTAIN]
ga:236   big(onChain.expires_at) <= BigInt(Date.now())  => {ms} vs {ms}                                 [CERTAIN]
ga:241   expiresAt: num(onChain.expires_at)             => {ms}                                         [CERTAIN]
ga:257   Math.max(600, Math.min(30 * 86_400, Math.round(opts.hours * 3600)))
         => {h} * {s/h} = {s}, clamped to [600 s, 30 d]                                                 [CERTAIN]
ga:264   createSession(keyHex, durationSecs, ...)       => arg {s} (program expects duration_secs)      [CERTAIN]
ga:272   BigInt(Math.round(opts.gasVara * 1e12))        => {VARA} * D12 = D12{VARA}                     [CERTAIN]
ga:284   Date.now() + durationSecs * 1000               => {ms} + {s} * {ms/s} = {ms}                   [CERTAIN]
ga:345   totalAssets = big(info.total_assets)           => D6{asset}                                    [CERTAIN]
ga:347   big(info.rate) / RATE_1E18_TO_1E9              => D18{asset/share} / D9 = D9{asset/share}      [CERTAIN]
         (documented D18→D9 cast; truncates 9 decimals, relative error < 1e-9; preview/display only)
ga:348   apyBps = BigInt(num(info.apy_bps))             => D4{1/yr}                                     [CERTAIN]
ga:349   instantFeeBps                                  => D4{1}                                        [CERTAIN]
ga:350   unbondSecs = num(info.unbond_period_secs)      => {s}                                          [CERTAIN]
ga:351   minDeposit                                     => D6{asset}                                    [CERTAIN]
ga:352-354 totalShares / totalAssets / holdings         => D6{share} / D6{asset} / D6{asset}            [CERTAIN]
ga:355   vestingEndsAt = num(info.vesting_ends_at)      => {ms}                                         [CERTAIN]
ga:356   (Number(totalAssets) / Number(ONE_STABLE)) * STABLE_PRICE_USD
         => D6{asset} / D6 * {USD/asset} = {USD}                                                        [CERTAIN]
ga:363   fallback: rate varaRateAt(now) D9{VARA/kVARA}; apyBps D4{1/yr}; staked D12{VARA};
         tvlUsd {USD}; feeBps 30n D4{1}; unbondSecs 7 * 86_400 {s}; reserve 0n D12{VARA}; vestingEndsAt Infinity {ms} [CERTAIN]
ga:365   staked = big(info.total_assets)                => D12{VARA}                                    [CERTAIN]
ga:367   big(info.rate) / RATE_1E18_TO_1E9              => D18{VARA/kVARA} / D9 = D9{VARA/kVARA}        [CERTAIN]
ga:369   apyBps                                         => D4{1/yr}                                     [CERTAIN]
ga:371   (Number(staked) / Number(ONE_VARA)) * VARA_PRICE_USD   => D12{VARA} / D12 * {USD/VARA} = {USD} [CERTAIN]
ga:373   unbondSecs                                     => {s}                                          [CERTAIN]
ga:374   reserve = big(info.reserve)                    => D12{VARA}                                    [CERTAIN]
ga:375   vestingEndsAt                                  => {ms}                                         [CERTAIN]
ga:387   pool.rate - (pool.rate * pool.apyBps * BigInt(n * BLOCK_SECS)) / (10_000n * 31_536_000n)
         => D9{a/s} - D9{a/s} * D4{1/yr} * ({block}*{s/block}={s}) / (D4 * {s/yr}) = D9{a/s}            [CERTAIN]
         (seconds against YEAR_SECS: consistent. One block at 3500 bps ≈ 33 D9 units ≈ 3.3e-8 rate)
ga:395   pool.tvlUsd + Σ vaults[a].tvlUsd               => {USD} + {USD} = {USD}                        [CERTAIN]
ga:397   (pool.reserve * 10_000n) / pool.staked         => D12{VARA} * D4 / D12{VARA} = D4{1}           [CERTAIN]
         (reserve ⊇ distributable, so this is ≥ 10 000 bps; see DIM-002 for the definition mismatch)
ga:399   now + BLOCK_SECS * 1000                        => {ms} + {s/block} * {ms/s} = {ms}             [CERTAIN]
ga:400   blockNumber - n                                => {block} - {1} = {block}                      [CERTAIN]
ga:429   amount big(u.assets) D6{asset}; startedAt/claimableAt num(ms)   => {ms}                        [INFERRED: UnbondEntry undocumented; formatCountdown consumes ms]
ga:433   amount D12{VARA}; startedAt/claimableAt        => {ms}                                         [INFERRED]
ga:445   amount D6{asset}; cooldownSecs {s}; nextClaimAt num(next)   => {ms}?                           [UNCERTAIN: demo-token program is out of scope]
ga:455   stake().withValue(vara)                        => D12{VARA}                                    [CERTAIN]
ga:456   shares = big(unwrap(r.result))                 => D12{kVARA}                                   [CERTAIN]
ga:463   net / fee                                      => D12{VARA}                                    [CERTAIN]
ga:470   amount = big(out.assets) D12{VARA}; claimableAt num(ms)   => {ms}                              [CERTAIN]
```

---

## scripts/deploy.ts  (tranche sizing and start rate)

```
deploy:28   ONE = 1_000_000n                            => D6 scale constant                            [CERTAIN]
deploy:31-32 apyBps: 790n / 840n                        => D4{1/yr}                                     [CERTAIN]
deploy:34   FAUCET_AMOUNT = 1_000n * ONE                => {asset} * D6 = D6{asset}                     [CERTAIN]
deploy:35   FAUCET_COOLDOWN_SECS = Number(args.cooldown ?? 6 * 3600)   => {h} * {s/h} = {s}             [CERTAIN]
deploy:36   INSTANT_FEE_BPS = 30                        => D4{1}                                        [CERTAIN]
deploy:37   UNBOND_SECS = Number(args.unbond ?? 7 * 86_400)   => {day} * {s/day} = {s}                  [CERTAIN]
deploy:39   VESTING_SECS = Number(args.vesting ?? 7 * 86_400) => {s}  (NOT clamped here; the programs clamp) [CERTAIN]
deploy:40   VAULT_FUNDING_VARA = BigInt(args.fund ?? 15) => {VARA} whole                                [CERTAIN]
deploy:42   SEED_TOKENS                                 => {asset} whole                                [CERTAIN]
deploy:44   POOL_APY_BPS = 3500n                        => D4{1/yr}                                     [CERTAIN]
deploy:45   SEED_STAKE_VARA                             => {VARA} whole                                 [CERTAIN]
deploy:46   ONE_VARA = 10n ** 12n                       => D12 scale constant                           [CERTAIN]
deploy:47   YEAR_SECS = 31_536_000n                     => {s/yr}                                       [CERTAIN]
deploy:53   (principal * apyBps * BigInt(VESTING_SECS)) / (10_000n * YEAR_SECS)
            => {asset} * D4{1/yr} * {s} / (D4 * {s/yr}) = {asset}  (same precision as principal)        [CERTAIN]
deploy:54   BigInt(args.rewards) * ONE_VARA              => {VARA} * D12 = D12{VARA}                     [CERTAIN]
deploy:54   tranche(SEED_STAKE_VARA * ONE_VARA, POOL_APY_BPS)   => tranche(D12{VARA}, D4{1/yr}) = D12{VARA} [CERTAIN]
deploy:61   RATE_SCALE_UP = 10n ** 9n                   => D9 scale factor                              [CERTAIN]
deploy:62   FRESH ? 0n : rate1e9 * RATE_SCALE_UP        => D9{asset/share} * D9 = D18{asset/share}; 0 => program clamps to 1e18 [CERTAIN]
deploy:66-67 GAS_INIT / GAS_CALL                        => {gas}                                        [CERTAIN]
deploy:89   Number(have) / 1e12, Number(need) / 1e12    => D12{VARA} / D12 = {VARA} (display)           [CERTAIN]
deploy:90   need + 10_000n * ONE_VARA                   => D12{VARA} + D12{VARA}                        [CERTAIN]
deploy:91   Number(need + 10_000n * ONE_VARA) / 1e12    => {VARA} (display)                             [CERTAIN]
deploy:99   Number(bal) / 1e12                          => {VARA} (display)                             [CERTAIN]
deploy:100  bal < 50n * ONE_VARA                        => D12{VARA} vs D12{VARA}                       [CERTAIN]
deploy:104  newCtorFromCode(..., 12, INSTANT_FEE_BPS, UNBOND_SECS, VESTING_SECS, startRate(varaRateAt(Date.now())))
            => decimals {1}=12; D4{1}; {s}; {s}; D18{VARA/kVARA}  — matches Program::new parameter dims [CERTAIN]
deploy:107  VAULT_FUNDING_VARA * ONE_VARA               => D12{VARA}                                    [CERTAIN]
deploy:109  SEED_STAKE_VARA * ONE_VARA + REWARDS_VARA + 100n * ONE_VARA   => D12{VARA} (all three)      [CERTAIN]
deploy:112  withValue(SEED_STAKE_VARA * ONE_VARA)       => D12{VARA}                                    [CERTAIN]
deploy:117  withValue(REWARDS_VARA)                     => D12{VARA}                                    [CERTAIN]
deploy:119  Number(REWARDS_VARA) / 1e12                 => {VARA} (display)                             [CERTAIN]
deploy:122  Number(poolInfo.total_assets|reserve|locked_rewards) / 1e12   => {VARA} (display)           [CERTAIN]
deploy:128  newCtorFromCode(tokenWasm, name, symbol, 6, FAUCET_AMOUNT, FAUCET_COOLDOWN_SECS)
            => decimals 6; D6{asset}; {s}                                                               [INFERRED: demo-token out of scope]
deploy:134  newCtorFromCode(..., 6, INSTANT_FEE_BPS, UNBOND_SECS, VESTING_SECS, startRate(stablePriceAt(a.asset, Date.now())))
            => D18{asset/share} start rate — matches VaultState::new                                    [CERTAIN]
deploy:139  VAULT_FUNDING_VARA * ONE_VARA               => D12{VARA}                                    [CERTAIN]
deploy:143  BigInt(rewardsArg) * ONE                    => D6{asset}                                    [CERTAIN]
deploy:143  tranche(SEED_TOKENS * ONE, a.apyBps)        => tranche(D6{asset}, D4{1/yr}) = D6{asset}     [CERTAIN]
deploy:147  amount = SEED_TOKENS * ONE                  => D6{asset}                                    [CERTAIN]
deploy:148  mint(.., amount + rewards) / :149 approve(.., amount + rewards)   => D6{asset} + D6{asset}  [CERTAIN]
deploy:151  deposit(amount)                             => D6{asset}                                    [CERTAIN]
deploy:156  fundRewards(rewards)                        => D6{asset}                                    [CERTAIN]
deploy:158  Number(rewards) / 1e6                       => {asset} (display)                            [CERTAIN]
deploy:163  Number(info.locked_rewards) / 1e6           => {asset} (display)                            [CERTAIN]
```

---

## Cross-file boundary checks (call sites verified)

| Boundary | Producer dimension | Consumer expectation | Status |
|----------|--------------------|----------------------|--------|
| `Program::new(initial_rate)` ← `deploy.startRate()` | D18{asset/share} | D18{asset/share} | OK |
| `PoolInfo.rate` / `VaultInfo.rate` → `ga:347/367` | D18 | divided by 1e9 → D9 | OK (documented cast) |
| `stats.rate`, `vaults[a].rate` → `math.ts` `rate`/`sharePrice` | D9 | D9 (`RATE_SCALE`) | OK |
| `apy_bps` (per 365 d year of 31 536 000 s) → `accrueRate` (`YEAR_MS`), `rateBlocksAgo` (`31_536_000n`), `deploy.tranche` (`YEAR_SECS`) | D4{1/yr}, 365-day year | same year length everywhere | OK |
| `vesting_ends_at` (ms) → `vestingEndsAt` → `projectRate(..., vestingEndsAt)` with `Date.now()` | {ms} | {ms} | OK |
| `unbond_period_secs` → `unbondSecs` → `StakePage` (`/ 86_400`, `/ 3600`) | {s} | {s} | OK |
| `Session.expires_at` (ms) ↔ `Date.now()` in `ga:217/236/284` | {ms} | {ms} | OK |
| `durationSecs` (s) → `create_session(duration_secs)` | {s} | {s} | OK |
| `Unbond.claimable_at` (ms) → `UnbondEntry.claimableAt` → `formatCountdown(ms)` | {ms} | {ms} | OK (INFERRED: field undocumented) |
| `VESTING_SECS` → `tranche()` and → `Program::new(vesting_period_secs)` | {s} unclamped | {s} clamped on chain to [3600, 31 536 000] | Mismatch when out of range → DIM-001 |
| `bufferBps` gear (`reserve/total_assets`) vs mock (`720n`, a 7.2 % buffer) | D4{1}, ≥ 10 000 | D4{1}, a small fraction | Definition mismatch → DIM-002 |
| `MIN_DEPOSIT = 1_000`, vault `decimals` (ctor arg) vs underlying VFT `decimals()` | assumes D6 | never checked | DIM-003 |

Totals: 273 annotated expressions/anchors — 267 CERTAIN, 5 INFERRED, 1 UNCERTAIN.
