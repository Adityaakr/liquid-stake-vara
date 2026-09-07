/**
 * Deploy the Vale Protocol programs (the kVARA pool, two demo tokens, two vaults) and wire them together.
 *
 *   DEPLOYER_SEED='<mnemonic or //Alice>' pnpm deploy [--rpc wss://rpc.vara.network] [--out deployments/mainnet.json]
 *
 * kVARA pool: upload + init, fund it with VARA for the messages it sends, optionally fund a rewards
 * reserve (--rewards VARA) and seed a stake (--stake VARA). Per stable asset: upload token code + init,
 * upload vault code + init (underlying = token), fund it, seed it and fund its first rewards tranche. Writes
 * program ids to the output file and prints the .env lines.
 *
 * Requires the programs to be built first:
 *   for p in demo-token vault vara-pool; do (cd programs/$p && cargo build --release); done
 */
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { GearApi, GearKeyring, decodeAddress } from '@gear-js/api';
import { DemoToken } from '../src/chain/idl/demo_token';
import { Vault } from '../src/chain/idl/vault';
import { VaraPool } from '../src/chain/idl/vara_pool';
import { stablePriceAt, varaRateAt } from '../src/chain/varaPool';

// `--flag value` or a bare `--flag` (also when the next token is another flag).
const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] === undefined || all[i + 1].startsWith('--') ? 'true' : all[i + 1]] : [])).filter((x) => x.length));
const RPC = args.rpc ?? process.env.VARA_RPC ?? 'wss://rpc.vara.network';
const OUT = args.out ?? (RPC.includes('127.0.0.1') || RPC.includes('localhost') ? 'deployments/local.json' : 'deployments/mainnet.json');
const SEED = process.env.DEPLOYER_SEED;
if (!SEED) { console.error('Set DEPLOYER_SEED to the deployer mnemonic, seed or //Alice for a local node.'); process.exit(1); }

const ONE = 1_000_000n; // 6 decimals
/** Per asset: names, and the APY the first rewards tranche is sized for (`--rewards-<asset>` overrides the amount). */
const ASSETS = [
  { asset: 'USDC', tokenName: 'Vale Demo USDC', vaultName: 'Vale kUSDC', apyBps: 790n },
  { asset: 'USDT', tokenName: 'Vale Demo USDT', vaultName: 'Vale kUSDT', apyBps: 840n },
] as const;
const FAUCET_AMOUNT = 1_000n * ONE;
const FAUCET_COOLDOWN_SECS = Number(args.cooldown ?? 6 * 3600);
const INSTANT_FEE_BPS = 30;
const UNBOND_SECS = Number(args.unbond ?? 7 * 86_400);
/** Each rewards tranche vests linearly over this long (the programs accept an hour to a year). */
const VESTING_SECS = Number(args.vesting ?? 7 * 86_400);
const VAULT_FUNDING_VARA = BigInt(args.fund ?? 15); // pays for the vault's outgoing messages
/** Whole tokens the deployer deposits into each vault after deploy so the pool is not empty. */
const SEED_TOKENS = BigInt(args.seed ?? 0);
/** kVARA pool: VARA the deployer stakes after deploy, and the APY its first rewards tranche is sized for. */
const POOL_APY_BPS = 3500n;
const SEED_STAKE_VARA = BigInt(args.stake ?? 0);
const ONE_VARA = 10n ** 12n;
const YEAR_SECS = 31_536_000n;
/**
 * Rewards that make one vesting period pay `apyBps` on `principal`: `principal × apy × vesting / year`.
 * The programs report the APY as the tranche's release rate over what holders own, so seeding with
 * this amount shows exactly the target APY until the tranche runs out.
 */
const tranche = (principal: bigint, apyBps: bigint) => (principal * apyBps * BigInt(VESTING_SECS)) / (10_000n * YEAR_SECS);
const REWARDS_VARA = args.rewards !== undefined ? BigInt(args.rewards) * ONE_VARA : tranche(SEED_STAKE_VARA * ONE_VARA, POOL_APY_BPS);
/**
 * Starting rates. By default each pool continues at the published rate for today (the app's
 * simulation runs on the same figures), scaled from 1e9 to the programs' 1e18. `--fresh` starts
 * every pool at exactly 1.0 (the edge-case suite relies on that).
 */
const FRESH = args.fresh === 'true';
const RATE_SCALE_UP = 10n ** 9n;
const startRate = (rate1e9: bigint) => (FRESH ? 0n : rate1e9 * RATE_SCALE_UP);
/** Optional env file to write (for example .env.local so `pnpm dev` runs against this deployment). */
const ENV_OUT = args.env;
const IS_LOCAL = RPC.includes('127.0.0.1') || RPC.includes('localhost');
const GAS_INIT = 150_000_000_000n;
const GAS_CALL = 30_000_000_000n;

const tokenWasm = readFileSync('programs/demo-token/target/wasm32-gear/release/demo_token.opt.wasm');
const vaultWasm = readFileSync('programs/vault/target/wasm32-gear/release/vault.opt.wasm');
const poolWasm = readFileSync('programs/vara-pool/target/wasm32-gear/release/vara_pool.opt.wasm');

type Api = Awaited<ReturnType<typeof GearApi.create>>;
type Pair = Awaited<ReturnType<typeof GearKeyring.fromSuri>>;

function signAndWait(tx: { signAndSend: (pair: Pair, cb: (r: { status: { isInBlock: boolean; isFinalized: boolean }; dispatchError?: unknown }) => void) => Promise<unknown> }, pair: Pair): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    tx.signAndSend(pair, ({ status, dispatchError }) => {
      if (dispatchError) reject(new Error(String(dispatchError)));
      if (status.isInBlock || status.isFinalized) resolve();
    }).catch(reject);
  });
}

/** Local dev chains only: give the deployer enough VARA through sudo to seed a large stake. */
async function ensureDevBalance(api: Api, pair: Pair, need: bigint) {
  const have = (await api.balance.findOut(pair.address)).toBigInt();
  if (have >= need) return;
  if (!IS_LOCAL) throw new Error(`deployer holds ${Number(have) / 1e12} VARA, needs ${Number(need) / 1e12}`);
  await signAndWait(api.tx.sudo.sudo(api.tx.balances.forceSetBalance(pair.address, need + 10_000n * ONE_VARA)), pair);
  console.log(`dev chain: raised the deployer balance to ${Number(need + 10_000n * ONE_VARA) / 1e12} VARA through sudo`);
}

async function main() {
  const api = await GearApi.create({ providerAddress: RPC });
  const pair = SEED.startsWith('//') ? await GearKeyring.fromSuri(SEED) : SEED.split(' ').length >= 12 ? await GearKeyring.fromMnemonic(SEED) : await GearKeyring.fromSeed(SEED);
  const chain = await api.chain();
  const bal = (await api.balance.findOut(pair.address)).toBigInt();
  console.log(`chain ${chain} · deployer ${pair.address} · balance ${Number(bal) / 1e12} VARA`);
  if (bal < 50n * ONE_VARA) console.warn('Balance looks low; five programs plus funding need roughly 50 VARA before any stake or rewards.');

  console.log('\n== kVARA pool');
  const pool = new VaraPool(api);
  const poolTx = pool.newCtorFromCode(poolWasm, 'Vale kVARA', 'kVARA', 12, INSTANT_FEE_BPS, UNBOND_SECS, VESTING_SECS, startRate(varaRateAt(Date.now()))).withAccount(pair).withGas(GAS_INIT);
  await (await poolTx.signAndSend()).response();
  console.log(`pool kVARA: ${pool.programId}`);
  await signAndWait(api.balance.transfer(pool.programId, VAULT_FUNDING_VARA * ONE_VARA), pair);
  console.log(`pool funded with ${VAULT_FUNDING_VARA} VARA for outgoing messages`);
  if (SEED_STAKE_VARA > 0n || REWARDS_VARA > 0n) await ensureDevBalance(api, pair, SEED_STAKE_VARA * ONE_VARA + REWARDS_VARA + 100n * ONE_VARA);
  // Stake first: rewards funded into an empty pool vest to nobody and are swept to fees.
  if (SEED_STAKE_VARA > 0n) {
    const r = await (await pool.pool.stake().withValue(SEED_STAKE_VARA * ONE_VARA).withAccount(pair).withGas(GAS_CALL).signAndSend()).response();
    if (!('ok' in r)) throw new Error(`seed stake failed: ${JSON.stringify(r)}`);
    console.log(`seeded the pool with ${SEED_STAKE_VARA.toLocaleString('en-US')} VARA from the deployer`);
  }
  if (REWARDS_VARA > 0n) {
    const r = await (await pool.pool.fundRewards().withValue(REWARDS_VARA).withAccount(pair).withGas(GAS_CALL).signAndSend()).response();
    if (!('ok' in r)) throw new Error(`fund rewards failed: ${JSON.stringify(r)}`);
    console.log(`rewards tranche funded with ${(Number(REWARDS_VARA) / 1e12).toLocaleString('en-US')} VARA, vesting over ${VESTING_SECS}s`);
  }
  const poolInfo = await pool.pool.info().call();
  console.log(`pool info: rate ${poolInfo.rate} apy ${poolInfo.apy_bps}bps unbond ${poolInfo.unbond_period_secs}s vesting ${poolInfo.vesting_period_secs}s staked ${Number(poolInfo.total_assets) / 1e12} VARA reserve ${Number(poolInfo.reserve) / 1e12} VARA locked ${Number(poolInfo.locked_rewards) / 1e12} VARA`);

  const out: Record<string, { token: string; vault: string }> = {};
  for (const a of ASSETS) {
    console.log(`\n== ${a.asset}`);
    const token = new DemoToken(api);
    const tokenTx = token.newCtorFromCode(tokenWasm, a.tokenName, a.asset, 6, FAUCET_AMOUNT, FAUCET_COOLDOWN_SECS).withAccount(pair).withGas(GAS_INIT);
    const t = await tokenTx.signAndSend();
    await t.response();
    console.log(`token ${a.asset}: ${token.programId}`);

    const vault = new Vault(api);
    const vaultTx = vault.newCtorFromCode(vaultWasm, token.programId, a.vaultName, `k${a.asset}`, 6, INSTANT_FEE_BPS, UNBOND_SECS, VESTING_SECS, startRate(stablePriceAt(a.asset, Date.now()))).withAccount(pair).withGas(GAS_INIT);
    const v = await vaultTx.signAndSend();
    await v.response();
    console.log(`vault k${a.asset}: ${vault.programId}`);

    await signAndWait(api.balance.transfer(vault.programId, VAULT_FUNDING_VARA * ONE_VARA), pair);
    console.log(`vault funded with ${VAULT_FUNDING_VARA} VARA for outgoing messages`);

    const rewardsArg = args[`rewards-${a.asset.toLowerCase()}`];
    const rewards = rewardsArg !== undefined ? BigInt(rewardsArg) * ONE : tranche(SEED_TOKENS * ONE, a.apyBps);
    if (SEED_TOKENS > 0n || rewards > 0n) {
      // The deployer mints demo tokens for the seed deposit and the first rewards tranche (rewards
      // are real tokens in the vault: the vault never mints).
      const amount = SEED_TOKENS * ONE;
      await (await token.admin.mint(decodeAddress(pair.address), amount + rewards).withAccount(pair).withGas(GAS_CALL).signAndSend()).response();
      await (await token.vft.approve(vault.programId, amount + rewards).withAccount(pair).withGas(GAS_CALL).signAndSend()).response();
      if (amount > 0n) {
        const seeded = await (await vault.vault.deposit(amount).withAccount(pair).withGas(100_000_000_000n).signAndSend()).response();
        if (!(typeof seeded === 'object' && seeded && 'ok' in seeded)) throw new Error(`seed deposit failed: ${JSON.stringify(seeded)}`);
        console.log(`seeded the vault with ${SEED_TOKENS.toLocaleString('en-US')} ${a.asset} from the deployer`);
      }
      if (rewards > 0n) {
        const funded = await (await vault.vault.fundRewards(rewards).withAccount(pair).withGas(100_000_000_000n).signAndSend()).response();
        if (!(typeof funded === 'object' && funded && 'ok' in funded)) throw new Error(`fund rewards failed: ${JSON.stringify(funded)}`);
        console.log(`rewards tranche funded with ${(Number(rewards) / 1e6).toLocaleString('en-US')} ${a.asset}, vesting over ${VESTING_SECS}s`);
      }
    }

    const info = await vault.vault.info().call();
    console.log(`vault info: rate ${info.rate} apy ${info.apy_bps}bps unbond ${info.unbond_period_secs}s vesting ${info.vesting_period_secs}s locked ${Number(info.locked_rewards) / 1e6} underlying ${info.underlying === decodeAddress(token.programId) || info.underlying === token.programId ? 'ok' : info.underlying}`);
    out[a.asset] = { token: token.programId, vault: vault.programId };
  }

  mkdirSync('deployments', { recursive: true });
  writeFileSync(OUT, JSON.stringify({ rpc: RPC, deployer: pair.address, deployedAt: new Date().toISOString(), pool: pool.programId, programs: out }, null, 2) + '\n');
  const envLines = ['VITE_ADAPTER=gear', `VITE_VARA_RPC=${RPC}`, `VITE_KVARA_POOL=${pool.programId}`];
  for (const a of ASSETS) {
    envLines.push(`VITE_${a.asset}_TOKEN=${out[a.asset].token}`);
    envLines.push(`VITE_K${a.asset}_VAULT=${out[a.asset].vault}`);
  }
  if (IS_LOCAL) envLines.push('# local dev chain only: lets the app hand out VARA for fees from the //Alice dev account', 'VITE_DEV_FUNDER_SURI=//Alice');
  console.log(`\nwrote ${OUT}\n\n.env lines:\n${envLines.join('\n')}`);
  if (ENV_OUT) { writeFileSync(ENV_OUT, envLines.join('\n') + '\n'); console.log(`\nwrote ${ENV_OUT}`); }
  await api.disconnect();
}

main().catch((e) => { console.error(e); process.exit(1); });
