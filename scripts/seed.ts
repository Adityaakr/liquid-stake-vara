/**
 * Top a vault up to a target TVL with deposits from the deployer (admin can mint demo tokens).
 *   DEPLOYER_SEED='//Alice' pnpm seed --rpc ws://127.0.0.1:9944 --usdc 356000 --usdt 761000
 * Targets are whole tokens. A vault already above its target is left alone.
 */
import { readFileSync } from 'node:fs';
import { GearApi, GearKeyring, decodeAddress } from '@gear-js/api';
import { DemoToken } from '../src/chain/idl/demo_token';
import { Vault } from '../src/chain/idl/vault';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] ?? 'true'] : [])).filter((x) => x.length));
const RPC = args.rpc ?? 'ws://127.0.0.1:9944';
const DEPLOYMENT = args.deployment ?? (RPC.includes('127.0.0.1') || RPC.includes('localhost') ? 'deployments/local.json' : 'deployments/mainnet.json');
const SEED = process.env.DEPLOYER_SEED ?? '//Alice';
const ONE = 1_000_000n;
const big = (v: unknown) => (typeof v === 'bigint' ? v : BigInt(String(v)));

async function main() {
  const dep = JSON.parse(readFileSync(DEPLOYMENT, 'utf8')) as { programs: Record<string, { token: `0x${string}`; vault: `0x${string}` }> };
  const api = await GearApi.create({ providerAddress: RPC });
  const pair = SEED.startsWith('//') ? await GearKeyring.fromSuri(SEED) : await GearKeyring.fromMnemonic(SEED);
  const me = decodeAddress(pair.address);
  for (const asset of ['USDC', 'USDT'] as const) {
    const target = args[asset.toLowerCase()];
    if (!target) continue;
    const token = new DemoToken(api, dep.programs[asset].token);
    const vault = new Vault(api, dep.programs[asset].vault);
    const info = await vault.vault.info().call();
    const have = big(info.total_assets);
    const want = BigInt(target) * ONE;
    if (have >= want) { console.log(`${asset}: TVL ${Number(have) / 1e6} already at or above ${target}`); continue; }
    const amount = want - have;
    const balance = big(await token.vft.balanceOf(me).call());
    if (balance < amount) await (await token.admin.mint(me, amount - balance).withAccount(pair).withGas(30_000_000_000n).signAndSend()).response();
    await (await token.vft.approve(dep.programs[asset].vault, amount).withAccount(pair).withGas(30_000_000_000n).signAndSend()).response();
    const r = await (await vault.vault.deposit(amount).withAccount(pair).withGas(100_000_000_000n).signAndSend()).response();
    if (!(typeof r === 'object' && r && 'ok' in r)) throw new Error(`${asset} deposit failed: ${JSON.stringify(r)}`);
    const after = big((await vault.vault.info().call()).total_assets);
    console.log(`${asset}: deposited ${Number(amount) / 1e6} → TVL ${Number(after) / 1e6}`);
  }
  await api.disconnect();
}
main().catch((e) => { console.error(e); process.exit(1); });
