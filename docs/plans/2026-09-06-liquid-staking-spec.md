# Vale Protocol on Vara mainnet: spec

Status: decided 2026-09-06. Owner request: "liquid staking protocol on Vara deployed on Vara
mainnet, 100% working and functional; for testing use USDC and USDT named demo tokens that a
user can deposit and get a 1:1 receipt token that provides yield, end to end."

## Goal

Ship a working liquid staking style vault on Vara mainnet that a user can exercise end to end
from the existing Vale Protocol app: get demo tokens, deposit, hold a transferable receipt token whose
value grows, exit instantly (small fee) or through a timed unbond (no fee).

Vara has no public testnet today, so the demo tokens and the protocol both live on mainnet.
Demo tokens have no monetary value; they exist to make the protocol testable.

## Users and flows

1. Visitor connects a Vara wallet (SubWallet, Polkadot.js, Talisman, Nova).
2. Visitor claims demo USDC or USDT from the token faucet (rate limited per address).
3. Visitor approves the vault and deposits N tokens. Receives shares of kUSDC or kUSDT.
   On day one the rate is exactly 1.0, so the receipt is 1:1.
4. Value accrues: the exchange rate rises continuously at the configured APY.
5. Visitor exits:
   - instant: burns shares, receives assets at the current rate minus the instant fee, or
   - unbond: burns shares, locks the assets at the current rate, claims after the unbond period.
6. Portfolio shows positions, rates, and pending unbonds.

## In scope

- Program `demo-token`: VFT standard token + admin roles + public faucet. Deployed twice
  (USDC, USDT), 6 decimals.
- Program `vault`: one instance per underlying. The vault program is itself the receipt token
  (its `Vft` service is the kUSDC or kUSDT token) and exposes a `Vault` service for deposit,
  redeem, unbond, claim, accrue, admin and queries.
- Frontend: the existing `/app` reads live state from the programs and signs real transactions.
  The simulation mode remains available behind `VITE_ADAPTER=mock` for local UI work.
- gtest coverage for every state transition, a local node smoke run, deploy scripts and docs.

## Out of scope for this iteration

- Native VARA staking through the staking built-in actor. The Stake page keeps the kVARA
  design but is labelled as coming soon on mainnet; nothing is simulated there.
- Borrowing against receipts, governance, indexers.

## Constraints and assumptions

- Vara mainnet runtime is spec 1.10.0 (checked 2026-09-06 via `vara-wallet node info`).
  Programs are built with `sails-rs 1.0.1` (gstd 1.10) to match; the newer sails-rs 2.0
  targets gear 2.0 and has no matching `sails-js` release.
- Yield on demo tokens has no external source. The vault holds the minter role on its
  underlying token and mints the accrued yield lazily on redemption when its holdings fall short.
  Deposits are always backed 1:1 by real balances; only the yield portion is minted.
- APY, instant fee and unbond period are admin configurable so the demo can show accrual
  in minutes rather than months.
- Deploying to mainnet costs real VARA (code upload, existential deposit per program, calls).
  The deployer account must be funded by the owner; nothing in the repo holds a seed.

## Acceptance

- `cargo test` is green in both program workspaces (unit + gtest).
- Local node smoke: deploy token + vault, faucet, approve, deposit, accrue, redeem, unbond,
  claim, all through the generated client.
- Mainnet: four programs deployed, ids recorded in `.env` and `deployments/mainnet.json`,
  the app on Vercel performs the full flow against them.
