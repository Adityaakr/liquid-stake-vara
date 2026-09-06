/**
 * Deploy the Vale Protocol programs (two demo tokens, two vaults) and wire them together.
 *
 *   DEPLOYER_SEED='<mnemonic or //Alice>' pnpm deploy [--rpc wss://rpc.vara.network] [--out deployments/mainnet.json]
 *
 * Steps per asset: upload token code + init, upload vault code + init (underlying = token),
 * grant the vault the minter role, transfer a little VARA to the vault so it can pay for the
 * messages it sends. Writes program ids to the output file and prints the .env lines.
 *
 * Requires the programs to be built first:
 *   (cd programs/demo-token && cargo build --release) && (cd programs/vault && cargo build --release)
 */
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { GearApi, GearKeyring, decodeAddress } from '@gear-js/api';
import { DemoToken } from '../src/chain/idl/demo_token';
import { Vault } from '../src/chain/idl/vault';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] ?? 'true'] : [])).filter((x) => x.length));
const RPC = args.rpc ?? process.env.VARA_RPC ?? 'wss://rpc.vara.network';
const OUT = args.out ?? (RPC.includes('127.0.0.1') || RPC.includes('localhost') ? 'deployments/local.json' : 'deployments/mainnet.json');
const SEED = process.env.DEPLOYER_SEED;
if (!SEED) { console.error('Set DEPLOYER_SEED to the deployer mnemonic, seed or //Alice for a local node.'); process.exit(1); }

const ONE = 1_000_000n; // 6 decimals
const ASSETS = [
  { asset: 'USDC', tokenName: 'Vale Demo USDC', vaultName: 'Vale kUSDC', apyBps: 790 },
  { asset: 'USDT', tokenName: 'Vale Demo USDT', vaultName: 'Vale kUSDT', apyBps: 840 },
] as const;
const FAUCET_AMOUNT = 1_000n * ONE;
const FAUCET_COOLDOWN_SECS = Number(args.cooldown ?? 6 * 3600);
const INSTANT_FEE_BPS = 30;
const UNBOND_SECS = Number(args.unbond ?? 7 * 86_400);
const VAULT_FUNDING_VARA = BigInt(args.fund ?? 15); // pays for the vault's outgoing messages
/** Whole tokens the deployer deposits into each vault after deploy so the pool is not empty. */
const SEED_TOKENS = BigInt(args.seed ?? 0);
/** Optional env file to write (for example .env.local so `pnpm dev` runs against this deployment). */
const ENV_OUT = args.env;
const IS_LOCAL = RPC.includes('127.0.0.1') || RPC.includes('localhost');
const GAS_INIT = 150_000_000_000n;
const GAS_CALL = 30_000_000_000n;

const tokenWasm = readFileSync('programs/demo-token/target/wasm32-gear/release/demo_token.opt.wasm');
const vaultWasm = readFileSync('programs/vault/target/wasm32-gear/release/vault.opt.wasm');

async function main() {
  const api = await GearApi.create({ providerAddress: RPC });
  const pair = SEED.startsWith('//') ? await GearKeyring.fromSuri(SEED) : SEED.split(' ').length >= 12 ? await GearKeyring.fromMnemonic(SEED) : await GearKeyring.fromSeed(SEED);
  const chain = await api.chain();
  const bal = (await api.balance.findOut(pair.address)).toBigInt();
  console.log(`chain ${chain} · deployer ${pair.address} · balance ${Number(bal) / 1e12} VARA`);
  if (bal < 40n * 10n ** 12n) console.warn('Balance looks low; four programs plus funding need roughly 40 VARA.');

  const out: Record<string, { token: string; vault: string }> = {};
  for (const a of ASSETS) {
    console.log(`\n== ${a.asset}`);
    const token = new DemoToken(api);
    const tokenTx = token.newCtorFromCode(tokenWasm, a.tokenName, a.asset, 6, FAUCET_AMOUNT, FAUCET_COOLDOWN_SECS).withAccount(pair).withGas(GAS_INIT);
    const t = await tokenTx.signAndSend();
    await t.response();
    console.log(`token ${a.asset}: ${token.programId}`);

    const vault = new Vault(api);
    const vaultTx = vault.newCtorFromCode(vaultWasm, token.programId, a.vaultName, `k${a.asset}`, 6, a.apyBps, INSTANT_FEE_BPS, UNBOND_SECS).withAccount(pair).withGas(GAS_INIT);
    const v = await vaultTx.signAndSend();
    await v.response();
    console.log(`vault k${a.asset}: ${vault.programId}`);

    const grant = await token.admin.grantMinter(vault.programId).withAccount(pair).withGas(GAS_CALL).signAndSend();
    const granted = await grant.response(); // throws on an error reply; `throws` functions come back unwrapped
    if (granted !== true && !(typeof granted === 'object' && granted && 'ok' in granted)) throw new Error(`grantMinter failed: ${JSON.stringify(granted)}`);
    console.log('vault granted minter role');

    await new Promise<void>((resolve, reject) => {
      api.balance.transfer(vault.programId, VAULT_FUNDING_VARA * 10n ** 12n).signAndSend(pair, ({ status, dispatchError }) => {
        if (dispatchError) reject(new Error(dispatchError.toString()));
        if (status.isInBlock || status.isFinalized) resolve();
      }).catch(reject);
    });
    console.log(`vault funded with ${VAULT_FUNDING_VARA} VARA for outgoing messages`);

    if (SEED_TOKENS > 0n) {
      const amount = SEED_TOKENS * ONE;
      await (await token.admin.mint(decodeAddress(pair.address), amount).withAccount(pair).withGas(GAS_CALL).signAndSend()).response();
      await (await token.vft.approve(vault.programId, amount).withAccount(pair).withGas(GAS_CALL).signAndSend()).response();
      const seeded = await (await vault.vault.deposit(amount).withAccount(pair).withGas(100_000_000_000n).signAndSend()).response();
      if (!(typeof seeded === 'object' && seeded && 'ok' in seeded)) throw new Error(`seed deposit failed: ${JSON.stringify(seeded)}`);
      console.log(`seeded the vault with ${SEED_TOKENS.toLocaleString('en-US')} ${a.asset} from the deployer`);
    }

    const info = await vault.vault.info().call();
    console.log(`vault info: rate ${info.rate} apy ${info.apy_bps}bps unbond ${info.unbond_period_secs}s underlying ${info.underlying === decodeAddress(token.programId) || info.underlying === token.programId ? 'ok' : info.underlying}`);
    out[a.asset] = { token: token.programId, vault: vault.programId };
  }

  mkdirSync('deployments', { recursive: true });
  writeFileSync(OUT, JSON.stringify({ rpc: RPC, deployer: pair.address, deployedAt: new Date().toISOString(), programs: out }, null, 2) + '\n');
  const envLines = ['VITE_ADAPTER=gear', `VITE_VARA_RPC=${RPC}`];
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
