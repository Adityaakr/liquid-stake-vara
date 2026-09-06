# Vale Protocol v1 architecture

Status: decided 2026-09-03 (Prism ship, lean pass). Gates G0 and G1 were cleared on stated
assumptions because the brief asked for an end-to-end build; every assumption is listed below
so it can be reversed cheaply.

## What ships

Two surfaces, one Vite app:

- `/` marketing landing, built 1:1 from the Claude Design kit `ui_kits/landing`.
- `/app`, `/app/vaults`, `/app/portfolio` the product, built from `ui_kits/app` and extended
  with the states a real protocol needs (wallet, pending tx, errors, empty, unbonding claims).

## Recommendation

| Layer | Choice | Why |
|---|---|---|
| Build | Vite 8, React 19, TypeScript 5.9 | The kit is React JSX. Vite is the fastest path to a static bundle that any CDN serves. |
| Routing | react-router 8 (`createBrowserRouter`) | One bundle, client routes, scroll restoration. |
| Styling | Kit tokens copied verbatim into `src/styles/tokens.css`, component CSS in `src/ui/ui.css` | Strict adherence to the design system; no Tailwind so the tokens stay the single source of truth. |
| Fonts | Clash Display, Clash Grotesk (Fontshare), JetBrains Mono (Google) | Specified by the kit. Loaded with `<link>` and preconnect instead of `@import`. |
| Icons | lucide-react 1.x | Kit specifies Lucide. |
| Chain | `@gear-js/api` 0.45 for RPC, `@polkadot/extension-dapp` for wallets, `sails-js` 1.0 for the program | Vara is a Gear chain; Sails is the program ABI standard. |
| State | React context + a `StakingAdapter` interface | No global store library needed at this size. |
| Tests | Vitest 4 + Testing Library | Domain math is unit tested, screens are smoke tested. |

## Assumptions (G0 defaults)

1. Vara mainnet only. RPC is `wss://rpc.vara.network`, overridable with `VITE_VARA_RPC`.
2. No staking program is deployed yet. The app runs on a `MockAdapter` that simulates the
   protocol exactly as the kit describes it (rate 1.0482, 14.2% APY, 0.3% instant fee, 7 day
   unbond). A `GearAdapter` reads real native VARA balances from mainnet and is wired to accept a
   program id and IDL through env vars when a program exists. The UI shows a "simulation" badge
   whenever the adapter is not talking to a real program.
3. Wallets: any Substrate injected extension (Polkadot.js, SubWallet, Talisman, Nova).
4. Token names exactly as the kit: `kVARA`, `kUSDT`, `kUSDC`. Wordmark `vale`.
5. No backend. Stats come from the adapter.
6. v1 scope: landing, stake, unstake (instant and native), claim unbonded, vaults deposit,
   portfolio. Deferred: borrowing, governance, analytics.

## Steelman of the rejected stack

Next.js with server rendering would give better SEO for the landing page. Rejected because the
app half is fully client side and wallet driven, the landing is a single route that pre-renders
fine as static HTML from Vite, and one build pipeline is simpler to host.

## Falsifiers

- If a Sails program exists with a different exchange rate model, the domain math in
  `src/domain/math.ts` is the one file to change.
- If Fontshare is unacceptable for production, self host the fonts under `public/fonts` and
  replace the two `<link>` tags.

## Open questions for the owner

- Program id and IDL for the staking program.
- Official Vara, Tether and Circle marks. The kit ships generated placeholders.
