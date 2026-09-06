# Vaultera on Vara mainnet: test report

Companion to the spec and architecture notes of the same date. Everything below was run on
2026-09-06 with `sails-rs 1.0.1`, `gtest 1.10.1`, and the `gear v1.10.0` node binary.

## demo-token

Host unit tests (`cargo test -p demo-token-app`): 5 passed. Mint, transfer, burn, allowance
spend including the `U256::MAX` infinite allowance, faucet cooldown, pause, roles.

gtest (`cargo test --test gtest`): 4 passed.

| Test | What it proves |
|---|---|
| `metadata_and_admin_are_set_by_constructor` | constructor args land in state; deployer is admin |
| `faucet_mints_and_enforces_cooldown` | claim mints, emits the VFT `Transfer` from the zero address, second claim returns the typed `FaucetCooldown`, works again after the cooldown blocks |
| `transfer_approve_transfer_from` | balances, allowance decrement, typed `InsufficientBalance` and `InsufficientAllowance` |
| `minter_role_and_pause` | unauthorized mint rejected, grant/revoke minter, pause blocks mints, faucet disable |

## vault

Host unit tests (`cargo test -p vault-app`): 11 passed. First deposit is 1:1, linear accrual
between checkpoints and compounding across them, zero APY and backwards clocks are inert,
deposit after growth mints fewer shares, redeem fee and burn, redeem revert restores shares and
fees, shortfall equals realised yield, unbond lock/claim/restore, minimum deposit and pause,
receipt transfers and allowances, info projection.

gtest (`cargo test --test gtest`, embeds the token wasm and exercises the cross-program calls): 8 passed.

| Test | What it proves |
|---|---|
| `constructor_state_and_metadata` | vault wired to the token, minter granted |
| `deposit_is_one_to_one_and_moves_underlying` | `TransferFrom` into the vault, shares minted 1:1, `Vft.Transfer` and `Vault.Deposited` events |
| `deposit_without_allowance_fails_without_side_effects` | token rejection surfaces as `TokenRejected(InsufficientAllowance)`, nothing minted; `BelowMinimum` and `ZeroAmount` before any transfer |
| `rate_accrues_and_redeem_pays_yield_by_minting_shortfall` | rate moves with blocks, redeem pays more than principal, token supply grows by exactly the realised yield |
| `unbond_then_claim_after_period` | entry recorded, early claim `UnbondNotReady`, wrong owner `UnbondNotFound`, claim pays after the period, no double claim |
| `receipt_token_is_transferable_and_redeemable_by_holder` | kUSDC moves between accounts and the new holder can redeem |
| `admin_controls` | config bounds, pause blocks deposits, fee collection mints its own shortfall, reserve top up, admin transfer |
| `accrue_checkpoints_and_emits` | public `Accrue` emits `Accrued` and persists the checkpoint |

## Local node smoke (gear v1.10.0, `--dev`)

Through `vara-wallet` and then through the app's own `GearAdapter` (`pnpm smoke`):
upload both programs, grant minter, fund the vault, faucet, approve, deposit, instant redeem,
request unbond, early claim rejected, claim after 60 s, below minimum rejected.

Finding fixed on the way: with the CLI's gas estimate a deposit ran out of gas *after* the token
transfer had completed, leaving tokens in the vault without shares. The vault now refuses to start
any async command unless the message carries enough gas for every segment (`NotEnoughGas`,
40B for one token call, 70B for two) and hands each token call an explicit 6B gas limit. The
frontend sends 100B for those commands; unused gas is refunded.

Costs observed on the local node: token upload ≈ 2.7 VARA, vault upload ≈ 3 VARA, calls a few
hundredths of a VARA each.

## Edge case suite on a fresh local node (2026-09-07, `pnpm edge`)

38 checks per vault, run against both kUSDC and kUSDT on a fresh deployment with 30 s unbond
and faucet cooldown: 38/38 and 38/38.

Deposit guards: no approval, exact approval consumed, above allowance, above wallet balance,
below minimum, zero, too little gas (typed `NotEnoughGas`, nothing moved), exact minimum,
1 billion USDC in and out without overflow.

Redeem and receipts: preview equals the actual payout, more shares than held, one share pays
one unit with zero fee, kUSDC transfer then redeem by the receiver, transfer above balance,
two redeems submitted back to back with consecutive nonces (one pays, the other gets
`InsufficientShares`, paid exactly once).

Yield and the mint fallback: 1000% APY moves the rate within seconds, a redeem whose payout
exceeds holdings mints exactly the shortfall, and when the vault has lost its minter role the
redeem fails with `TokenRejected(Unauthorized)` and restores the shares and fees, then succeeds
once the role is back. Fee collection pays the admin, `Accrue` is callable by anyone.

Unbonding: locked amount fixed at request time and does not accrue, claim before maturity,
claim by another account, claim after maturity, double claim.

Pause and admin: non-admin rejected on config, pause, admin transfer and fee collection;
config bounds; paused vault rejects deposit, redeem, unbond and receipt transfers; paused
token makes deposit fail with no shares minted; reserve top up; admin handover and back;
faucet disable and re-enable.

The app adapter (`GearAdapter`) was run on the same deployment for reads and a deposit, and
`pnpm ui-check` rendered the built app against the node in Chromium: live badges, rate, APY and
block number from chain, no simulation badge, coming soon panel on the stake page.

A vault deployed with no VARA beyond its existential deposit completed a deposit and a
mint-backed redeem, so the per-vault funding in the deploy script is only a safety margin.

Observed cost per user action on the local node: faucet claim 0.29 VARA, approve 0.28,
deposit 0.81, redeem 0.81, request unbond 0.33. A deposit cost the same with a 60B and a 100B
gas limit, so the app keeps the 100B margin for the two-call flows and deposits alike.
