# Vale Protocol on Vara mainnet: architecture

Companion to `2026-09-06-liquid-staking-spec.md`.

## Programs

Two Sails workspaces under `programs/`, one program each (one workspace per program, as the
Sails template requires so `gstd` and `gtest` features never unify).

### `demo-token` (deployed twice: USDC, USDT)

| Service | Routes | Notes |
|---|---|---|
| `Vft` | `Approve`, `Transfer`, `TransferFrom`, `Allowance`, `BalanceOf`, `TotalSupply`, `Name`, `Symbol`, `Decimals`; events `Approval`, `Transfer` | Exactly the VFT standard so wallets and `vara-wallet vft` work. |
| `Admin` | `Mint`, `Burn` (admin or minter), `GrantMinter`, `RevokeMinter`, `SetFaucet`, `TransferAdmin`, `Pause`, `Resume`; queries `Admin`, `Minters`, `IsMinter`, `IsPaused` | Single admin plus a minter set. The vault is granted minter. |
| `Faucet` | `Claim`; queries `Config`, `NextClaimAt` | Public, one claim per address per cooldown. |

State: program owned `RefCell<TokenState>` (balances and allowances in `BTreeMap<_, U256>`).
Pure state transitions live on `TokenState` and are unit tested on the host; services are thin.

Why not `awesome-sails-vft`: its balances are 80 bit compact integers in sharded maps with
allowance expiry by block height. That is built for very large token holder sets and adds
invariants (dust burns, shard capacity, expiring approvals) that the vault's share accounting
would have to reason about. A 200 line VFT over `U256` with the same routes and events keeps
the two programs' accounting identical and reviewable. The public surface is unchanged.

### `vault` (deployed twice: kUSDC over USDC, kUSDT over USDT)

The vault program is the receipt token (ERC-4626 shape: the vault mints and burns its own
shares). Services:

| Service | Routes |
|---|---|
| `Vft` | The standard surface for the receipt token; balances are share balances. |
| `Vault` | `Deposit(assets)`, `Redeem(shares)`, `RequestUnbond(shares)`, `Claim(id)`, `Accrue()`, admin `SetApy`, `SetInstantFee`, `SetUnbondPeriod`, `Pause`, `Resume`, `TopUpReserve`, `CollectFees`, `TransferAdmin`; queries `Info`, `PreviewDeposit`, `PreviewRedeem`, `Position`, `Unbonds`. |

Accounting (all `U256`, rate scaled by `1e18`):

- `rate` = assets per share. Starts at `1e18`. On every state changing call (and on `Accrue`)
  the rate is checkpointed: `rate += rate * apy_bps * dt_secs / (10_000 * 31_536_000)`.
  Compounding therefore happens per interaction; between interactions it is simple interest.
- deposit: `shares = assets * 1e18 / rate` (rounded down). Requires `assets >= 1_000` base
  units so rounding cannot produce zero shares at any plausible rate.
- redeem: `assets = shares * rate / 1e18`, `fee = assets * fee_bps / 10_000`, user gets
  `assets - fee`; the fee stays in the vault as protocol fees (`fees_accrued`, admin collects).
- unbond: shares burned now, `assets` fixed at the current rate, claimable at
  `now + unbond_period`. No fee.
- `holdings` = underlying the vault physically holds (deposits in, payouts out, top ups).
  `total_assets = total_shares * rate / 1e18 + unbonding_total + fees_accrued`.
  When a payout exceeds holdings the vault mints the shortfall to itself first
  (`Admin.Mint` on the underlying, vault is a minter). That shortfall is exactly realised yield.

Async safety rules (Gear persists state at every `.await`):

1. Validate and checkpoint before the first `.await`. Never hold a `RefCell` borrow across it.
2. Burn shares or remove the unbond entry before the outbound transfer so a re-entrant call
   cannot spend twice. If the transfer fails, compensate (re-mint, re-insert) and return `Err`.
3. Async commands return `Result<_, VaultError>` without `unwrap_result`. A panic after an
   `.await` would only revert the resumed segment, not the pre-await burn, so failures after
   the await must be handled by returning an error reply, never by panicking.
4. Sync commands use `#[export(unwrap_result)]` so failures revert.

Cross-program calls use the Rust client generated from `demo_token.idl`
(`programs/vault/app/src/token_client.rs`, regenerated with `cargo sails client-rs`).

## Frontend

- `GearAdapter` becomes real: `sails-js` `Program` classes generated from both IDLs
  (`src/chain/idl/*.ts` via `cargo sails client-js`), wallet signer from
  `@polkadot/extension-dapp`, gas from `calculateGas()` with a safety margin.
- Env contract: `VITE_VARA_RPC`, `VITE_ADAPTER=gear`, `VITE_USDC_TOKEN`, `VITE_USDT_TOKEN`,
  `VITE_KUSDC_VAULT`, `VITE_KUSDT_VAULT`. Missing ids show a visible "not deployed" state; the
  UI never silently simulates on mainnet.
- Screens: Vaults page is the live product (faucet, deposit, redeem instant, unbond, claim).
  Stake page (VARA) is marked coming soon. Portfolio reads shares, rate, unbonds from chain.

## Deployment

`scripts/deploy-mainnet.mjs` (Node, `@gear-js/api` + generated clients) uploads code, creates
the four programs, grants the vault minter role, configures faucets and writes
`deployments/mainnet.json`. The deployer seed is read from `DEPLOYER_SEED` in the shell,
never from a file in the repo. Local smoke uses the same script against `ws://127.0.0.1:9944`
with the `//Alice` dev seed.

## Steelman of the rejected designs

- Rebasing receipt (balance grows): more intuitive "1:1 forever" story but breaks DEX and
  wallet integrations and doubles the token logic. Rate based shares are the industry default
  (wstETH, 4626) and match the app's existing math.
- Monolithic program (tokens + vault in one): atomic but the tokens would not be standalone
  VFTs, so wallets would not show them. Rejected.
- Real VARA staking via the built-in staking actor: the real product, but it moves real
  value and needs validator selection, era accounting and a 7 day unbond that cannot be
  demonstrated quickly. Planned as phase 2 behind the same `Vault` interface.
