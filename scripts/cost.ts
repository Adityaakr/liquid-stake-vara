/**
 * Measure what each user action costs in VARA on a node (fees + burned gas, refunds included).
 *   SEED='//Charlie' node node_modules/tsx/dist/cli.mjs --tsconfig tsconfig.app.json scripts/cost.ts --rpc ws://127.0.0.1:9944 --deployment deployments/local.json
 * The account must already hold VARA.
 */
import { readFileSync } from 'node:fs';
import { GearApi, GearKeyring } from '@gear-js/api';
import type { TransactionBuilderWithHeader } from 'sails-js';
import { DemoToken } from '../src/chain/idl/demo_token';
import { Vault } from '../src/chain/idl/vault';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] ?? 'true'] : [])).filter((x) => x.length));
const RPC = args.rpc ?? 'ws://127.0.0.1:9944';
const dep = JSON.parse(readFileSync(args.deployment ?? 'deployments/local.json', 'utf8'));
const ASSET = (args.asset ?? 'USDT') as 'USDC' | 'USDT';

async function main() {
  const api = await GearApi.create({ providerAddress: RPC });
  const seed = process.env.SEED ?? '//Charlie';
  const who = seed.startsWith('//') ? await GearKeyring.fromSuri(seed) : await GearKeyring.fromMnemonic(seed);
  const token = new DemoToken(api, dep.programs[ASSET].token);
  const vault = new Vault(api, dep.programs[ASSET].vault);
  const bal = async () => (await api.balance.findOut(who.address)).toBigInt();
  console.log(`account ${who.address} · balance ${Number(await bal()) / 1e12} VARA`);
  async function cost(label: string, b: TransactionBuilderWithHeader<unknown>, gas: bigint) {
    const before = await bal();
    const { response } = await b.withAccount(who).withGas(gas).signAndSend();
    await response();
    const after = await bal();
    console.log(`${label.padEnd(18)} ${(Number(before - after) / 1e12).toFixed(4)} VARA  (limit ${gas / 1_000_000_000n}B gas)`);
  }
  await cost('faucet claim', token.faucet.claim(), 30_000_000_000n);
  await cost('approve', token.vft.approve(dep.programs[ASSET].vault, 2n ** 256n - 1n), 30_000_000_000n);
  await cost('deposit 100', vault.vault.deposit(100_000_000n), 100_000_000_000n);
  await cost('redeem 10', vault.vault.redeem(10_000_000n), 100_000_000_000n);
  await cost('request unbond 10', vault.vault.requestUnbond(10_000_000n), 40_000_000_000n);
  await api.disconnect();
}
main().catch((e) => { console.error(e); process.exit(1); });
