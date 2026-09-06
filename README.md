# Vale Protocol

Liquid staking on Vara Network. Deposit an asset, receive a transferable receipt token
(kVARA, kUSDT, kUSDC) whose value grows with the pool's rate, and exit instantly or through
an unbond at the full rate.

Live at https://liquid-stake.vercel.app.

## What is in the box

- `programs/demo-token` a Vara Fungible Token (VFT) with admin roles and a public faucet.
  Deployed twice on mainnet as demo USDC and demo USDT. Demo tokens have no monetary value.
- `programs/vault` the liquid staking vault. The program is itself the receipt token: its
  `Vft` service is kUSDC or kUSDT, its `Vault` service moves the underlying in and out.
- The app: marketing site at `/`, product at `/app` with stake and unstake for every asset
  from one screen, the pools overview with live positions and earnings, a portfolio with
  unbonding claims, and a settings screen with the demo token faucet.
- Wallets through the official Vara account provider and wallet modal (SubWallet,
  Polkadot.js, Talisman, Enkrypt, Nova).

Native VARA staking (kVARA) is shown as coming soon on mainnet; the stable pools are live.

## Run the app

```sh
pnpm install
cp .env.example .env      # program ids from deployments/mainnet.json
pnpm dev                  # http://localhost:5173
pnpm test                 # vitest: domain math, simulation, screens
pnpm build                # tsc + vite build -> dist/
```

| Variable | Default | Meaning |
|---|---|---|
| `VITE_VARA_RPC` | `wss://rpc.vara.network` | Vara mainnet RPC. |
| `VITE_ADAPTER` | `gear` | `gear` talks to the programs on Vara. `mock` runs a simulation in the browser. |
| `VITE_USDC_TOKEN`, `VITE_KUSDC_VAULT`, `VITE_USDT_TOKEN`, `VITE_KUSDT_VAULT` | empty | Program ids. Without all four the pools show as not configured; nothing is simulated on mainnet. |

## Try it with demo tokens

Anyone holding a little VARA for fees can use the protocol end to end:

1. Connect a wallet in the app.
2. Open **Settings** and claim demo USDT or USDC from the faucet (1,000 per claim, once per cooldown).
3. Deposit in a pool. The first deposit signs twice: an approval for the vault, then the deposit.
4. Watch the rate accrue, then withdraw instantly (0.3% fee) or start an unbond and claim later.

## Programs

Built with `sails-rs 1.0.1` (gstd 1.10), which matches the Vara mainnet runtime (spec 1.10.0).
Toolchain: `rustup` with the `wasm32v1-none` target, `cargo install sails-cli --version 1.0.1 --locked`,
`brew install binaryen` for `wasm-opt`.

```sh
(cd programs/demo-token && cargo build --release && cargo test --release)   # unit + gtest
(cd programs/vault && cargo build --release && cargo test --release)        # unit + gtest (embeds the token wasm)
scripts/gen-clients.sh            # refresh src/chain/idl/*.idl and the TypeScript clients
```

Artifacts: `programs/*/target/wasm32-gear/release/*.opt.wasm` and `*.idl`.

### Demo token (`Vft`, `Admin`, `Faucet`)

The VFT standard routes and events, a single admin plus a minter set (each vault is a minter of
its underlying so it can mint realised yield), and a faucet that mints a fixed amount per address
per cooldown.

### Vault (`Vft`, `Vault`)

Rate based shares, ERC-4626 shape. `rate` is assets per share scaled by 1e18, starts at 1.0 so
the first deposit is 1:1, accrues at the configured APY every second and is checkpointed on each
state changing call.

- `Deposit(assets)` pulls the underlying with `TransferFrom` and mints shares.
- `Redeem(shares)` burns shares and pays assets at the current rate minus the instant fee. When
  holdings fall short the vault mints the shortfall, which is exactly the realised yield.
- `RequestUnbond(shares)` burns shares and locks the assets at the current rate; `Claim(id)`
  pays after the unbond period.
- Async commands refuse to start unless the message carries enough gas for every segment
  (`NotEnoughGas`), so a deposit can never move tokens without minting shares.
- Admin: `SetConfig(apy_bps, fee_bps, unbond_secs)`, `Pause`, `Resume`, `TopUpReserve`,
  `CollectFees`, `TransferAdmin`.

Every user facing command replies with a typed result, so failures reach the app as readable
messages.

## Deploy to mainnet

```sh
DEPLOYER_SEED='<mnemonic>' pnpm deploy            # writes deployments/mainnet.json and prints the .env lines
DEPLOYER_SEED='<mnemonic>' pnpm seed --usdc 230000 --usdt 577000   # optional: deposit up to a target TVL
```

The deploy script uploads both programs per asset, grants each vault the minter role, funds it
with VARA for the messages it sends, and prints the `.env` lines. Budget about 40 VARA for four
programs plus funding. The seed is read from the environment only. Gas: commands that call the
token program are sent with 100B gas, the rest with 40B or 30B; a deposit or redeem costs about
0.8 VARA on a 1.10 node.

## Verification before a deployment

```sh
scripts/local.sh                                  # dev node, seeded programs, .env.local, app on :5173
SMOKE_SEED='//Alice' pnpm smoke --rpc ws://127.0.0.1:9944    # the app's adapter end to end
pnpm edge --rpc ws://127.0.0.1:9944 [--asset USDT]            # 38 edge cases on a fresh deployment (--unbond 30 --cooldown 30)
pnpm ui-check --rpc ws://127.0.0.1:9944                        # the built app in a browser against the node
scripts/local.sh stop
```

The test report and the design notes live in `docs/plans`.

## Layout

```
programs/      Sails workspaces (demo-token, vault), one program each
src/chain      StakingAdapter interface, GearAdapter (sails-js clients), simulation, store
src/chain/idl  IDL snapshots and generated TypeScript clients
src/domain     protocol constants, bigint math, formatting (unit tested)
src/app        app shell and screens
src/landing    marketing site
scripts        deploy, seed, smoke, edge cases, client generation, screenshots
docs/plans     spec, architecture and test report
```
