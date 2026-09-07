# Vale Protocol

Liquid staking on Vara Network. Deposit an asset, receive a transferable receipt token
(kVARA, kUSDT, kUSDC) whose value grows as the pool's rewards vest, and exit instantly or through
an unbond at the full rate. The rate is what the pool holds divided by the receipts in circulation:
nothing is promised that is not there, and yield is earned only for the time a receipt is held.

Live at https://liquid-stake.vercel.app.

## What is in the box

- `programs/vara-pool` the kVARA pool: native VARA staking. Stake VARA with the message,
  receive kVARA; unstake instantly or through an unbond. The program is itself the receipt
  token: its `Vft` service is kVARA, its `Pool` service moves VARA in and out.
- `programs/demo-token` a Vara Fungible Token (VFT) with admin roles and a public faucet.
  Deployed twice on mainnet as demo USDC and demo USDT. Demo tokens have no monetary value.
- `programs/vault` the stable vault. The program is itself the receipt token: its `Vft`
  service is kUSDC or kUSDT, its `Vault` service moves the underlying in and out.
- The app: marketing site at `/`, product at `/app` with stake and unstake for every asset
  from one screen, the pools overview with live positions and earnings, a portfolio with
  unbonding claims, and a settings screen with the demo token faucet and one-click sessions.
- Wallets through the official Vara account provider and wallet modal (SubWallet,
  Polkadot.js, Talisman, Enkrypt, Nova).

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
| `VITE_KVARA_POOL` | empty | The kVARA pool program id. Without it the kVARA card shows as not configured. |
| `VITE_USDC_TOKEN`, `VITE_KUSDC_VAULT`, `VITE_USDT_TOKEN`, `VITE_KUSDT_VAULT` | empty | Stable pool program ids. Without all four the stable pools show as not configured; nothing is simulated on mainnet. |

## Stake VARA

Connect a wallet, pick VARA on the stake screen and stake. The VARA travels with the message,
kVARA lands in the wallet at the current rate, and the rate rises every block as the current
rewards tranche vests. Exit instantly (0.3% fee, paid from the pool reserve) or start an unbond
and claim at the full rate after the unbond period. kVARA is a transferable VFT.

Staking and leaving in the same block earns nothing: a receipt only collects the slice of every
reward that vested while it was held, and the instant fee makes a quick round trip a small loss.

## Try it with demo tokens

Anyone holding a little VARA for fees can use the stable pools end to end:

1. Connect a wallet in the app.
2. Open **Settings** and claim demo USDT or USDC from the faucet (1,000 per claim, once per cooldown).
3. Deposit in a pool. The first deposit signs twice: an approval for the vault, then the deposit.
4. Watch the rate accrue, then withdraw instantly (0.3% fee) or start an unbond and claim later.

## One-click transactions

Open **Settings** and enable a session. That is one wallet signature which bundles: unlimited
approvals for both vaults, a session proposed to a fresh key in each vault and in the kVARA pool,
and 15 VARA moved to that key for fees. The key then accepts each proposal itself (signed locally,
no prompt): a session only ever binds a key that agreed to it, so nobody can register somebody
else's account as their key. From then on deposits, withdrawals, unstakes, unbonds and claims
are signed locally by the key and go straight to chain, each with a link to the transaction. The
key lives only in that browser and can only operate your own position: every payout goes to your
account. Sessions expire (up to 30 days) and can be revoked at any time, which returns the key's
leftover VARA. Staking VARA and faucet claims still ask the wallet, because the VARA comes from
your account and the token mints to whoever signs.

Vara reserves gas at 100 units per gas unit while a message runs, so a session key needs at least
11 VARA to send a vault command; below that the app falls back to wallet signing until you enable
a new session.

## Programs

Built with `sails-rs 1.0.1` (gstd 1.10), which matches the Vara mainnet runtime (spec 1.10.0).
Toolchain: `rustup` with the `wasm32v1-none` target, `cargo install sails-cli --version 1.0.1 --locked`,
`brew install binaryen` for `wasm-opt`.

```sh
(cd programs/demo-token && cargo build --release && cargo test --release)   # unit + gtest
(cd programs/vault && cargo build --release && cargo test --release)        # unit + gtest (embeds the token wasm)
(cd programs/vara-pool && cargo build --release && cargo test --release)    # unit + gtest
scripts/gen-clients.sh            # refresh src/chain/idl/*.idl and the TypeScript clients
```

Artifacts: `programs/*/target/wasm32-gear/release/*.opt.wasm` and `*.idl`.

### Accounting: earned yield only

Both pools keep the same books. `rate` (assets per receipt, scaled by 1e18) is
`distributable / total_shares`, where `distributable` is what the program holds minus collected
fees, minus assets locked for unbonds, minus rewards that are still vesting. There is no stored
rate and no clock: the rate moves only when real assets move.

- Rewards arrive through `FundRewards` (anyone may fund) and vest linearly over the vesting
  period (an hour to a year, 7 days on mainnet). A new tranche folds in what is still locked, and
  its length is the amount-weighted average of the time left and a full period (Yearn v3 style),
  so dust funding cannot stretch a running tranche and a fresh tranche gets a full period.
- A receipt therefore earns exactly the slice of every reward that vested while it was held.
  Depositing and leaving in the same block pays back the deposit minus the instant fee; one block
  later it adds one block of the pro-rata drip, far below the fee.
- `apy_bps` in `Info()` is derived: the vesting tranche's release rate, annualised, over
  distributable assets. It is zero once the tranche has fully vested, until the next funding.
- Shares and payouts round down (in the pool's favour). An empty pool keeps its last rate for the
  next deposit; assets that belong to nobody (dust, rewards vested while empty) are swept into the
  fee bucket rather than handed to the next depositor.
- Sessions: `CreateSession(key, duration_secs, actions)` only proposes; the key sends
  `AcceptSession(owner)` to activate it. `RevokeSession()` drops both. Queries `Session(owner)`,
  `PendingSession(owner)`, `SessionOwner(key)`.

### kVARA pool (`Vft`, `Pool`)

The books above over native VARA. Everything is synchronous: value moves with the message, so
there are no cross-program calls and no partial states.

- `Stake()` is payable: the attached VARA is the deposit, kVARA is minted at the current rate.
  On any error the attached value comes back with the reply.
- `Unstake(shares)` burns kVARA and sends VARA at the current rate minus the instant fee.
- `RequestUnbond(shares)` locks VARA at the current rate and burns the kVARA; `Claim(id)` pays
  after the unbond period. Locked VARA no longer earns.
- `FundRewards()` is payable: the attached VARA is the next rewards tranche. This is where staking
  rewards flow in. A payout the distributable reserve cannot cover is refused with
  `InsufficientReserve` and the position stays intact.
- Admin: `SetConfig(fee_bps, unbond_secs, vesting_secs)`, `Pause`, `Resume`, `CollectFees`,
  `TransferAdmin`. The session allow-list for the pool is unstake, unbond and claim, since staking
  needs the owner's own VARA.

### Demo token (`Vft`, `Admin`, `Faucet`)

The VFT standard routes and events, a single admin plus a minter set, and a faucet that mints a
fixed amount per address per cooldown. The vaults are not minters: their rewards are tokens
somebody funded.

### Vault (`Vft`, `Vault`)

The books above over one underlying VFT, ERC-4626 shape. The first deposit is priced at the
constructor's `initial_rate` (0 means 1.0, so it is 1:1); a vault that continues an earlier one
starts at that vault's last rate, and from then on the rate is what the vault holds over its
shares. `pnpm deploy` uses the published rates for the day unless `--fresh` is passed.

- `Deposit(assets)` pulls the underlying with `TransferFrom` and mints shares once the tokens
  arrived; in-flight deposits are invisible to the rate until then.
- `Redeem(shares)` burns shares, takes the payout out of holdings before the transfer, and pays
  assets at the current rate minus the instant fee. A failed transfer restores everything.
- `RequestUnbond(shares)` burns shares and locks the assets at the current rate; `Claim(id)`
  pays after the unbond period.
- `FundRewards(assets)` pulls the underlying from the caller (requires approval) into the next
  vesting tranche. Anyone may fund.
- Async commands refuse to start unless the message carries enough gas for every segment
  (`NotEnoughGas`), so a deposit can never move tokens without minting shares.
- Admin: `SetConfig(fee_bps, unbond_secs, vesting_secs)`, `Pause`, `Resume`, `CollectFees`,
  `TransferAdmin`.

Every user facing command replies with a typed result, so failures reach the app as readable
messages.

## Deploy to mainnet

```sh
DEPLOYER_SEED='<mnemonic>' pnpm deploy [--stake 500] [--rewards 10] [--seed 1000] [--vesting 604800]   # writes deployments/mainnet.json and prints the .env lines
DEPLOYER_SEED='<mnemonic>' pnpm seed --usdc 230000 --usdt 577000 --vara 500   # optional: top pools up to a target
```

The deploy script uploads the kVARA pool, then both programs per stable asset, funds every
program with VARA for the messages it sends, seeds each pool from the deployer (`--stake` VARA,
`--seed` demo tokens per vault) and funds a first rewards tranche sized so one vesting period pays
the published APY on the seed (35% kVARA, 8.4% kUSDT, 7.9% kUSDC); `--rewards`,
`--rewards-usdt` and `--rewards-usdc` set the amounts directly. Rewards are real assets: on
mainnet the VARA tranche comes out of the deployer's balance, so budget for it on top of about 50
VARA for five programs plus funding. The seed is read from the environment only. Gas: vault commands that call the token program are sent with 100B gas,
pool commands and the rest with 40B or 30B; a deposit or redeem costs about 0.8 VARA on a 1.10
node, a stake or unstake less.

## Verification before a deployment

```sh
scripts/local.sh                                  # dev node, seeded programs (pool included), .env.local, app on :5173
SMOKE_SEED='//Alice' pnpm smoke --rpc ws://127.0.0.1:9944    # the app's adapter end to end: stake, unstake, vaults
pnpm edge --rpc ws://127.0.0.1:9944 [--asset USDT]            # edge cases on a fresh deployment (pnpm deploy --fresh --unbond 30 --cooldown 30 --vesting 3600 --out deployments/edge.json)
pnpm ui-check --rpc ws://127.0.0.1:9944                        # the built app in a browser against the node
pnpm session-smoke --rpc ws://127.0.0.1:9944                   # one-click session: enable, act with the key, revoke
scripts/local.sh stop
```

The test report and the design notes live in `docs/plans`.

## Layout

```
programs/      Sails workspaces (vara-pool, demo-token, vault), one program each
src/chain      StakingAdapter interface, GearAdapter (sails-js clients), simulation, store
src/chain/idl  IDL snapshots and generated TypeScript clients
src/domain     protocol constants, bigint math, formatting (unit tested)
src/app        app shell and screens
src/landing    marketing site
scripts        deploy, seed, smoke, edge cases, client generation, screenshots
docs/plans     spec, architecture and test report
```
