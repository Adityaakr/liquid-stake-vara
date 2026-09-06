/**
 * Drive the deployed programs through the app's own GearAdapter with a keyring signer.
 *
 *   SMOKE_SEED='//Alice' pnpm smoke [--rpc ws://127.0.0.1:9944] [--deployment deployments/local.json]
 *
 * Proves the exact code path the browser uses (generated clients, gas limits, error mapping)
 * against a real node: faucet, deposit, redeem, unbond, claim.
 */
import { readFileSync } from 'node:fs';
import { GearKeyring } from '@gear-js/api';
import { GearAdapter } from '../src/chain/gearAdapter';
import type { VaultAsset } from '../src/domain/protocol';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] ?? 'true'] : [])).filter((x) => x.length));
const RPC = args.rpc ?? 'ws://127.0.0.1:9944';
const DEPLOYMENT = args.deployment ?? 'deployments/local.json';
const SEED = process.env.SMOKE_SEED ?? '//Alice';
const ASSET = (args.asset ?? 'USDC') as VaultAsset;
const ONE = 1_000_000n;

function must(cond: unknown, msg: string) { if (!cond) throw new Error(`smoke failed: ${msg}`); }

async function main() {
  const dep = JSON.parse(readFileSync(DEPLOYMENT, 'utf8')) as { programs: Record<VaultAsset, { token: `0x${string}`; vault: `0x${string}` }> };
  const pair = SEED.startsWith('//') ? await GearKeyring.fromSuri(SEED) : await GearKeyring.fromMnemonic(SEED);
  const adapter = new GearAdapter('mainnet', { programs: dep.programs, rpc: RPC, signer: async () => ({ account: pair }) });
  const me = pair.address;
  console.log(`account ${me} · rpc ${RPC} · asset ${ASSET}`);

  const stats = await adapter.getStats();
  const v = stats.vaults[ASSET];
  console.log(`vault rate ${v.rate} apy ${v.apyBps}bps fee ${v.instantFeeBps}bps unbond ${v.unbondSecs}s tvl $${v.tvlUsd} block ${stats.blockNumber}`);

  let f = (await adapter.getFaucet(me))[ASSET];
  let b = await adapter.getBalances(me);
  console.log(`balances ${ASSET} ${b[ASSET]} k${ASSET} ${b[`k${ASSET}`]} · faucet next ${f.nextClaimAt}`);
  if (f.nextClaimAt <= Date.now()) {
    const r = await adapter.claimFaucet(me, ASSET);
    console.log(`faucet claimed in block ${r.blockNumber}`);
  } else {
    console.log('faucet cooling down, skipping claim');
  }
  b = await adapter.getBalances(me);
  must(b[ASSET] >= 100n * ONE, `need at least 100 ${ASSET} to continue, have ${b[ASSET]}`);

  const before = b;
  const d = await adapter.depositVault(me, ASSET, 100n * ONE);
  console.log(`deposited 100 ${ASSET} → ${d.shares} k${ASSET} (block ${d.blockNumber})`);
  b = await adapter.getBalances(me);
  must(b[ASSET] === before[ASSET] - 100n * ONE, 'underlying debited');
  must(b[`k${ASSET}`] === before[`k${ASSET}`] + d.shares!, 'shares credited');

  const half = d.shares! / 2n;
  const rd = await adapter.redeemVault(me, ASSET, half);
  console.log(`redeemed ${half} shares → net ${rd.net} fee ${rd.fee}`);
  const afterRedeem = await adapter.getBalances(me);
  must(afterRedeem[ASSET] === b[ASSET] + rd.net!, 'net paid out');

  const ub = await adapter.unbondVault(me, ASSET, half / 2n);
  console.log(`unbond requested: ${ub.amount} ${ASSET} claimable at ${ub.claimableAt}`);
  const list = await adapter.getUnbonding(me);
  const entry = list.find((u) => u.claimableAt === ub.claimableAt);
  must(entry, 'unbond entry listed');

  try {
    await adapter.claimUnbond(me, ASSET, entry!.id);
    must(Date.now() >= entry!.claimableAt, 'early claim must fail');
  } catch (e) {
    console.log(`early claim rejected as expected: ${(e as Error).message}`);
  }
  const waitMs = entry!.claimableAt - Date.now();
  if (waitMs > 0 && waitMs <= 5 * 60_000) {
    console.log(`waiting ${Math.ceil(waitMs / 1000)}s for the unbond to mature`);
    await new Promise((r) => setTimeout(r, waitMs + 4000));
    const c = await adapter.claimUnbond(me, ASSET, entry!.id);
    console.log(`claimed in block ${c.blockNumber}`);
    const final = await adapter.getBalances(me);
    must(final[ASSET] === afterRedeem[ASSET] + ub.amount!, 'unbond paid out');
  } else {
    console.log(`unbond period is ${Math.round(waitMs / 1000)}s; leaving the entry for a later claim`);
  }

  try {
    await adapter.depositVault(me, ASSET, 1n);
    must(false, 'below-minimum deposit must fail');
  } catch (e) {
    console.log(`below-minimum deposit rejected as expected: ${(e as Error).message}`);
  }

  f = (await adapter.getFaucet(me))[ASSET];
  console.log(`done · faucet next ${new Date(f.nextClaimAt).toISOString()}`);
  await adapter.dispose();
}

main().catch((e) => { console.error(e); process.exit(1); });
