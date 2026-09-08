# logic-1: Share-supply can be driven to 1 and then priced with permissionless `fund_rewards`, so every later stake is rounded down to whole multiples of the attacker's number (ERC-4626 inflation attack with no offset, and the damage is permanent)

**Location**
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:371-383` (`PoolState::unstake`: burns any `shares >= 1` with no lower bound on the shares that remain), `:396-413` + `:760-776` (`fund` / `Pool::fund_rewards`: anyone may add to `reserve` and to the vesting tranche), `:245-250` (`shares_for`: `assets * total_shares / distributable`, floor), `:336-343` (`burn_shares` freezes the inflated rate into `base_rate` when the last share goes), `:29-30` (the `MIN_STAKE` comment claims "share rounding can never produce zero").
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:398-410` (`begin_redeem`, same shape), `:423-440` + `:826-842` (`fund` / `Vault::fund_rewards`), `:266-271` (`shares_for`), `:378-385` (`check_deposit` rejects `shares == 0`, i.e. the effective minimum deposit becomes the inflated per-share price), `:357-364` (`burn_shares` / `base_rate`).

**Bug class**: Logic / rounding direction (first-depositor share-price inflation), permissionless donation vector.

**Severity**: High (pool: theft of up to ~50% of a victim's stake plus permanent loss of share precision; vault: same, and with the faucet-minted demo token the attacker's capital is free, so the vault can be bricked at zero cost).

**Description**
`MIN_STAKE` / `MIN_DEPOSIT` bound the *deposit*, not the *share supply that remains after an exit*. `unstake` / `begin_redeem` accept any `shares >= 1` and only require `net > 0`, so a holder can burn all but one share. `total_shares == 1` is a legal, non-empty state (`empty()` is false because `distributable > 0`), and from then on `shares_for(assets) = assets * 1 / distributable` (floor).

`fund_rewards` is permissionless and adds the attached value to `reserve` and to the vesting tranche. Once the tranche has vested (`MIN_VESTING_SECS = 3600`), `distributable = 1 + X` with `total_shares = 1`, so one share is worth `X + 1` base units and every subsequent stake is rounded down to a whole multiple of `X + 1`. The remainder is not refunded: it is added to `reserve` and belongs pro rata to the existing shares (i.e. to the attacker while they are the main holder). `stake` still passes its own checks because `shares >= 1` as long as `assets >= X + 1`; `check_deposit` in the vault rejects anything smaller (`BelowMinimum`), which turns the inflated price into a hard minimum deposit.

Nothing ever lowers the rate again: `rate_at` is non-decreasing across `stake` / `unstake` / `request_unbond` (all round in the pool's favour), and when the last share is burned `burn_shares` copies the inflated rate into `base_rate`, which is then used for the next "first" stake (`shares_for` empty branch: `assets * SCALE / base_rate`). There is no admin method that resets `base_rate`, `total_shares`, or the vesting tranche, so the precision damage survives the attacker leaving and can only be undone by redeploying.

Exact code path (pool, fee 30 bps, 12 decimals):
1. Attacker `stake` with `MIN_STAKE = 1e10` -> `shares_for` empty branch -> `1e10` shares. `total_shares = 1e10`, `reserve = 1e10`.
2. Attacker `unstake(1e10 - 1)`: `assets_for = 1e10 - 1`, `fee = 29_999_997`, `net = 9_970_000_002 > 0`, `assets <= distributable` -> ok. After `burn_shares`: `total_shares = 1`, `reserve = 29_999_998`, `fees_accrued = 29_999_997`, `distributable = 1`.
3. Attacker `fund_rewards` with `X = 1e15` (1000 VARA). `fund`: `remaining = 0` -> `time_left = vesting_period`, `vest_amount = X`, `reserve = X + 29_999_998`.
4. After `vest_end_ms`: `locked_rewards = 0`, `distributable = X + 1`, `total_shares = 1`, `rate_at = (X + 1) * 1e18`.
5. Victim `stake` with `V = 1.999e15` (1999 VARA): `shares_for = floor(V * 1 / (X + 1)) = 1` -> passes the `shares.is_zero()` check, mints 1 share. Now `total_shares = 2`, `distributable = X + 1 + V`.
6. Victim `position(...)`: `assets_for(1) = (X + 1 + V) / 2 = 1.4995e15`; the victim has lost ~499.5 VARA of the 1999 staked. The attacker's single share is now worth the same 1.4995e15.
7. Attacker `request_unbond(1)` -> `assets = 1.4995e15` (no fee), `claim` after the unbond period. Net gain ~499.5 VARA.
8. Alternatively the attacker exits fully at step 4 (`unstake(1)` -> `base_rate = (X + 1) * 1e18`, `total_shares = 0`). Every later staker is priced at `assets * 1e18 / base_rate`, i.e. whole multiples of 1000 VARA, forever.

Vault: identical with `MIN_DEPOSIT = 1000`, `begin_redeem(999)` (fee 2, net 997, `total_shares = 1`, `distributable = 1`), then `fund_rewards(X)`. Because the underlying is the demo token whose `Faucet::claim` mints freely, `X` can be `1e30`; after one hour `check_deposit` returns `BelowMinimum` for every realistic deposit (`shares_for == 0`) and the vault takes no new deposits ever again.

**Concrete trigger**
Pool: `stake{value:1e10}`; `unstake(9_999_999_999)`; `fund_rewards{value:1e15}`; wait 3600 s; any third-party `stake{value: 1.999e15}` loses `~4.995e14` to the attacker's share. Vault: `deposit(1000)`; `redeem(999)`; `fund_rewards(1e15)`; wait one hour; `deposit(1.999e15)` loses `~4.995e14`; or `fund_rewards(1e30)` to block all deposits.

**Impact**
Any account can (a) take up to ~50% of every later stake/deposit whose amount is not an exact multiple of the inflated share price, and (b) permanently reduce share granularity for the whole deployment at a cost of only gas + the instant fee on `MIN_STAKE - 1` (or zero fee via `request_unbond`), since the "reward" capital is recovered through their own share. The 1-hour vesting only exposes the attacker to honest stakers arriving during that hour; it does not prevent the attack. The admin has no recovery path short of redeploying.

**Confidence**: high. Reproduced on the host with a throwaway `#[test]` on `PoolState` (fee 30 bps, vesting 3600 s): after `stake(MIN_STAKE)`, `unstake(MIN_STAKE - 1)`, `fund(1e15)` and one hour, `stake(1.999e15)` returned exactly 1 share, `assets_for(1) < 1.599e15`, the attacker's share was worth `> 1.4e15`, `base_rate` after both exits exceeded `500 VARA * SCALE`, and a subsequent `stake(500 VARA)` returned `BelowMinimum`.

**Suggested fix**
- Add a virtual-share / decimals offset (OpenZeppelin ERC-4626 style): compute `shares_for` as `assets * (total_shares + 10^k) / (distributable + 1)` and `assets_for` as `shares * (distributable + 1) / (total_shares + 10^k)`, so the per-share price cannot be pushed far above `1 / 10^k` no matter how small `total_shares` is.
- Or mint a dead-share stake on the very first deposit (e.g. `MIN_STAKE` shares to the program itself, never burnable), and refuse any burn that would leave `0 < total_shares < MIN_SHARES`.
- Independently, gate `fund_rewards` behind the admin or an allow-listed funder, so donation-based price manipulation requires a trusted party.
- Add a unit test that drives `total_shares` to 1, funds, vests, and asserts that a second staker's `assets_for` is within one base unit of their stake.
