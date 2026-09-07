/**
 * One-click session flow through the app's GearAdapter with keyring signers:
 * enable (one batched signature), deposit/redeem/unbond signed by the session key, revoke.
 *   pnpm tsx --tsconfig tsconfig.app.json scripts/session-smoke.ts --rpc ws://127.0.0.1:9944 --deployment deployments/local.json
 */
import { readFileSync } from 'node:fs';
import { GearApi, GearKeyring } from '@gear-js/api';
import { GearAdapter } from '../src/chain/gearAdapter';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] ?? 'true'] : [])).filter((x) => x.length));
const RPC = args.rpc ?? 'ws://127.0.0.1:9944';
const DEPLOYMENT = args.deployment ?? 'deployments/local.json';
const ONE = 1_000_000n;
function must(cond: unknown, msg: string): asserts cond { if (!cond) throw new Error(`session smoke failed: ${msg}`); }

// Node has no localStorage; give the adapter a tiny in-memory one.
const mem = new Map<string, string>();
(globalThis as unknown as { localStorage: Storage }).localStorage = {
  getItem: (k: string) => mem.get(k) ?? null, setItem: (k: string, v: string) => { mem.set(k, v); }, removeItem: (k: string) => { mem.delete(k); }, clear: () => mem.clear(), key: () => null, length: 0,
} as Storage;

async function main() {
  const dep = JSON.parse(readFileSync(DEPLOYMENT, 'utf8')) as { programs: Record<'USDC' | 'USDT', { token: `0x${string}`; vault: `0x${string}` }> };
  const api = await GearApi.create({ providerAddress: RPC });
  const alice = await GearKeyring.fromSuri('//Alice');
  const bob = await GearKeyring.fromSuri('//Bob');
  if ((await api.balance.findOut(bob.address)).toBigInt() < 100n * 10n ** 12n) {
    await new Promise<void>((res) => { api.balance.transfer(bob.address, 300n * 10n ** 12n).signAndSend(alice, ({ status }) => { if (status.isInBlock) res(); }); });
  }
  const adapter = new GearAdapter('mainnet', { programs: dep.programs, rpc: RPC, signer: async () => ({ account: bob }) });
  const me = bob.address;

  must((await adapter.getSession(me)) === null, 'no session at start');
  if ((await adapter.getFaucet(me)).USDC.nextClaimAt <= Date.now()) await adapter.claimFaucet(me, 'USDC');
  const before = await adapter.getBalances(me);
  console.log(`bob USDC ${before.USDC} kUSDC ${before.kUSDC} VARA ${Number(before.VARA) / 1e12}`);

  const on = await adapter.enableSession(me, { hours: 1, gasVara: Number(args.gas ?? 15) });
  console.log('session enabled in one batched tx', on.hash, 'key', on.session.key);
  const s1 = await adapter.getSession(me);
  must(s1 && s1.key === on.session.key && s1.gasBalance >= 11n * 10n ** 12n, `session not visible: ${s1 ? `${s1.key} gas ${s1.gasBalance}` : "null"}`);
  must(s1.actions.length === 4, `actions ${s1.actions}`);

  // Deposit signed by the session key: shares land on Bob, the key pays the fee.
  const bobVaraBefore = (await api.balance.findOut(me)).toBigInt();
  const d = await adapter.depositVault(me, 'USDC', 100n * ONE);
  const after = await adapter.getBalances(me);
  must(after.kUSDC === before.kUSDC + d.shares!, 'shares credited to bob');
  must(after.USDC === before.USDC - 100n * ONE, 'usdc debited from bob');
  const bobVaraAfter = (await api.balance.findOut(me)).toBigInt();
  must(bobVaraAfter === bobVaraBefore, `bob paid a fee (${bobVaraBefore - bobVaraAfter}); the session key should have`);
  const s2 = await adapter.getSession(me);
  must(s2 && s2.gasBalance < s1.gasBalance, 'session key paid the fee');
  console.log(`deposit via session: +${d.shares} kUSDC, key gas ${Number(s2!.gasBalance) / 1e12} VARA`);

  const r = await adapter.redeemVault(me, 'USDC', d.shares! / 2n);
  must((await adapter.getBalances(me)).USDC === after.USDC + r.net!, 'redeem paid bob');
  const u = await adapter.unbondVault(me, 'USDC', d.shares! / 4n);
  must((await adapter.getUnbonding(me)).some((x) => x.claimableAt === u.claimableAt), 'unbond listed');
  console.log('redeem and unbond via session ok');

  const off = await adapter.revokeSession(me);
  console.log('session revoked', off.hash);
  must((await adapter.getSession(me)) === null, 'session gone');
  const keyLeft = (await api.balance.findOut(on.session.key)).toBigInt();
  must(keyLeft === 0n, `key still holds ${keyLeft}`);
  const bobVaraFinal = (await api.balance.findOut(me)).toBigInt();
  must(bobVaraFinal > bobVaraAfter, 'leftover VARA returned to bob');
  console.log(`done · bob got ${Number(bobVaraFinal - bobVaraAfter) / 1e12} VARA back from the key`);
  await adapter.dispose();
  await api.disconnect();
}
main().catch((e) => { console.error(e); process.exit(1); });
