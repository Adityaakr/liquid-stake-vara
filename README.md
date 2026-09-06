# Vale Protocol

Liquid staking on Vara Network. Stake VARA or deposit USDT and USDC, receive a transferable
receipt token (kVARA, kUSDT, kUSDC) whose value grows with the pool's rate, and exit instantly
or through a native unbond.

Live at https://liquid-stake.vercel.app.

## What is in the box

- `/` the marketing site: product story, features, use cases, security, pricing and FAQs.
- `/app` the product: stake and unstake any of the three assets from one screen, the vaults
  overview with live positions and earnings, and a portfolio with unbonding claims.
- A wallet flow built on the official Vara account provider and wallet modal
  (SubWallet, Polkadot.js, Talisman, Enkrypt, Nova).

The app targets Vara mainnet. Program writes run through the simulated protocol until the
staking programs are configured, and the interface says so with a "simulation" badge.

## Run it

```sh
pnpm install
pnpm dev          # http://localhost:5173
pnpm test         # vitest: domain math, protocol simulation, screens
pnpm build        # tsc + vite build -> dist/
pnpm preview
```

Copy `.env.example` to `.env` to change defaults.

| Variable | Default | Meaning |
|---|---|---|
| `VITE_VARA_RPC` | `wss://rpc.vara.network` | Vara mainnet RPC endpoint. |
| `VITE_ADAPTER` | `gear` | `gear` reads native VARA balances from mainnet; `mock` runs fully in the browser. |
| `VITE_VALE_PROGRAM_ID` | empty | Sails program id. Until it is set, protocol writes are simulated. |

Deployment: a static Vite build. `vercel.json` rewrites every path to `index.html` so deep
links resolve to the app.

## How the protocol works

Rate based receipts, the same shape as wstETH or an ERC-4626 vault. Each pool has an exchange
rate between its receipt and its asset; the rate starts at 1.0 and accrues at the pool's APY,
so a receipt is worth more every second it is held. No rebasing, no claiming.

| Pool | Receipt | Exit |
|---|---|---|
| kVARA / VARA | kVARA | instant with a 0.3% fee, or native unbond over 7 days at the full rate |
| kUSDT / USDT | kUSDT | instant with a 0.3% fee |
| kUSDC / USDC | kUSDC | instant with a 0.3% fee |

All amounts are bigint in base units (VARA has 12 decimals, stables 6). Rates are scaled by
1e9. Mint: `receipts = assets / rate`. Redeem: `assets = receipts × rate`.

## Layout

```
design/        imported design kit (reference only)
docs/          architecture notes
src/styles     design tokens and global styles
src/ui         design system primitives
src/landing    marketing site
src/app        app shell and screens
src/domain     protocol constants, bigint math, formatting (unit tested)
src/chain      StakingAdapter interface, simulation, Vara adapter, store
scripts        screenshot generators for the marketing pages
```

## Scripts

```sh
node scripts/app-shots.mjs     # regenerate the dashboard images the landing page embeds (after pnpm build)
node scripts/screenshots.mjs   # screenshots of every screen into ./screenshots
```
