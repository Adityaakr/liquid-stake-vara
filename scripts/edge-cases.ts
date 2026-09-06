/**
 * Pre-mainnet edge case suite. Runs against a node with a fresh deployment (deployments/*.json)
 * using two funded dev accounts, through the generated clients so gas limits and error shapes are
 * exactly what the app will see.
 *
 *   node node_modules/tsx/dist/cli.mjs --tsconfig tsconfig.app.json scripts/edge-cases.ts \
 *     --rpc ws://127.0.0.1:9944 --deployment deployments/local.json
 *
 * Expects the deployment to use short periods (`pnpm deploy --unbond 30 --cooldown 30`).
 */
import { readFileSync } from 'node:fs';
import { GearApi, GearKeyring, decodeAddress, type HexString } from '@gear-js/api';
import type { KeyringPair } from '@polkadot/keyring/types';
import type { TransactionBuilderWithHeader } from 'sails-js';
import { DemoToken } from '../src/chain/idl/demo_token';
import { Vault } from '../src/chain/idl/vault';
import { GearAdapter, describeProgramError } from '../src/chain/gearAdapter';
import type { VaultAsset } from '../src/domain/protocol';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] ?? 'true'] : [])).filter((x) => x.length));
const RPC = args.rpc ?? 'ws://127.0.0.1:9944';
const DEPLOYMENT = args.deployment ?? 'deployments/local.json';
const ASSET = (args.asset ?? 'USDC') as VaultAsset;
const ONE = 1_000_000n;
const G100 = 100_000_000_000n;
const G40 = 40_000_000_000n;
const G30 = 30_000_000_000n;
const SCALE = 10n ** 18n;

type Res<T> = { ok: T } | { err: unknown };
const results: { name: string; ok: boolean; note?: string }[] = [];
function pass(name: string, note?: string) { results.push({ name, ok: true, note }); console.log(`  ✓ ${name}${note ? ` · ${note}` : ''}`); }
function fail(name: string, note: string) { results.push({ name, ok: false, note }); console.log(`  ✗ ${name} · ${note}`); }
async function check(name: string, fn: () => Promise<string | void>) {
  try { const note = await fn(); pass(name, note ?? undefined); } catch (e) { fail(name, (e as Error).message); }
}
function must(cond: unknown, msg: string): asserts cond { if (!cond) throw new Error(msg); }
const big = (v: unknown) => (typeof v === 'bigint' ? v : BigInt(String(v)));
const hexOf = (a: string): HexString => decodeAddress(a);

async function send<T>(b: TransactionBuilderWithHeader<T>, who: KeyringPair, gas = G100): Promise<T> {
  b.withAccount(who).withGas(gas);
  const { response } = await b.signAndSend();
  return response();
}
function errOf(r: unknown): string | null {
  if (r && typeof r === 'object' && 'err' in r) return describeProgramError((r as { err: unknown }).err);
  return null;
}
function okOf<T>(r: Res<T> | T): T {
  const e = errOf(r);
  if (e) throw new Error(`unexpected error: ${e}`);
  return r && typeof r === 'object' && 'ok' in (r as object) ? (r as { ok: T }).ok : (r as T);
}
async function expectErr(p: Promise<unknown>, pattern: RegExp): Promise<string> {
  let r: unknown;
  try { r = await p; } catch (e) { const m = (e as Error).message; must(pattern.test(m), `expected /${pattern.source}/, threw: ${m}`); return m; }
  const e = errOf(r);
  must(e && pattern.test(e), `expected /${pattern.source}/, got ${JSON.stringify(r, (_, v) => (typeof v === 'bigint' ? v.toString() : v))}`);
  return e!;
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function main() {
  const dep = JSON.parse(readFileSync(DEPLOYMENT, 'utf8')) as { programs: Record<VaultAsset, { token: HexString; vault: HexString }> };
  const ids = dep.programs[ASSET];
  const api = await GearApi.create({ providerAddress: RPC });
  const alice = await GearKeyring.fromSuri('//Alice');
  const bob = await GearKeyring.fromSuri('//Bob');
  const A = hexOf(alice.address);
  const B = hexOf(bob.address);
  const token = new DemoToken(api, ids.token);
  const vault = new Vault(api, ids.vault);
  const V = ids.vault;
  const bal = async (who: HexString) => big(await token.vft.balanceOf(who).call());
  const shares = async (who: HexString) => big(await vault.vft.balanceOf(who).call());
  const info = async () => vault.vault.info().call();
  console.log(`asset ${ASSET} · token ${ids.token} · vault ${V}`);

  // Bob needs VARA for fees on a dev chain.
  const bobBal = (await api.balance.findOut(bob.address)).toBigInt();
  if (bobBal < 100n * 10n ** 12n) {
    await new Promise<void>((res, rej) => { api.balance.transfer(bob.address, 500n * 10n ** 12n).signAndSend(alice, ({ status, dispatchError }) => { if (dispatchError) rej(new Error(String(dispatchError))); if (status.isInBlock) res(); }).catch(rej); });
  }

  console.log('\n# setup');
  await check('admin sets a zero APY so share math is exact', async () => {
    okOf(await send(vault.vault.setConfig(0, 30, 30), alice, G40));
    const i = await info();
    must(Number(i.apy_bps) === 0 && Number(i.unbond_period_secs) === 30, `config ${i.apy_bps} ${i.unbond_period_secs}`);
  });
  await check('faucet claims for both accounts', async () => {
    okOf(await send(token.faucet.claim(), alice, G30));
    okOf(await send(token.faucet.claim(), bob, G30));
    must((await bal(A)) >= 1_000n * ONE && (await bal(B)) >= 1_000n * ONE, 'balances after faucet');
  });
  await check('faucet cooldown is a typed error', async () => expectErr(send(token.faucet.claim(), alice, G30), /cooling down/));

  console.log('\n# deposit guards');
  await check('deposit without approval moves nothing', async () => {
    const b0 = await bal(B); const s0 = await shares(B); const h0 = big((await info()).holdings);
    const m = await expectErr(send(vault.vault.deposit(10n * ONE), bob), /not approved|InsufficientAllowance/i);
    must((await bal(B)) === b0 && (await shares(B)) === s0 && big((await info()).holdings) === h0, 'state changed');
    return m;
  });
  await check('approve exact amount, then deposit exactly that amount', async () => {
    okOf(await send(token.vft.approve(V, 10n * ONE), bob, G30));
    const s0 = await shares(B);
    const expected = big(await vault.vault.previewDeposit(10n * ONE).call());
    const got = big(okOf(await send(vault.vault.deposit(10n * ONE), bob)));
    must(got === expected, `shares ${got} expected ${expected}`);
    must((await shares(B)) === s0 + got, 'shares credited');
    must(big(await token.vft.allowance(B, V).call()) === 0n, 'allowance consumed');
  });
  await check('deposit above allowance is rejected cleanly', async () => expectErr(send(vault.vault.deposit(1n * ONE), bob), /not approved|InsufficientAllowance/i));
  await check('unlimited approval for the rest of the suite', async () => {
    okOf(await send(token.vft.approve(V, 2n ** 256n - 1n), alice, G30));
    okOf(await send(token.vft.approve(V, 2n ** 256n - 1n), bob, G30));
  });
  await check('deposit above wallet balance is rejected by the token', async () => {
    const b = await bal(B);
    return expectErr(send(vault.vault.deposit(b + 1n), bob), /Not enough balance|InsufficientBalance/i);
  });
  await check('deposit below the minimum and zero are rejected before any transfer', async () => {
    await expectErr(send(vault.vault.deposit(999n), bob), /minimum/i);
    await expectErr(send(vault.vault.deposit(0n), bob), /more than zero/i);
  });
  await check('deposit with too little gas is refused and moves nothing', async () => {
    const b0 = await bal(B); const s0 = await shares(B);
    const m = await expectErr(send(vault.vault.deposit(5n * ONE), bob, 20_000_000_000n), /gas/i);
    must((await bal(B)) === b0 && (await shares(B)) === s0, 'state changed');
    return m;
  });
  await check('exact minimum deposit works', async () => {
    const expected = big(await vault.vault.previewDeposit(1_000n).call());
    const got = big(okOf(await send(vault.vault.deposit(1_000n), bob)));
    must(got === expected && got > 0n, `shares ${got} expected ${expected}`);
  });
  await check('large deposit and redeem have no overflow (1 billion USDC)', async () => {
    okOf(await send(token.admin.mint(A, 1_000_000_000n * ONE), alice, G30));
    const expected = big(await vault.vault.previewDeposit(1_000_000_000n * ONE).call());
    const got = big(okOf(await send(vault.vault.deposit(1_000_000_000n * ONE), alice)));
    must(got === expected, `shares ${got} expected ${expected}`);
    must(big((await info()).total_assets) >= 1_000_000_000n * ONE - 1n, 'total assets');
    const a0 = await bal(A);
    const out = okOf(await send(vault.vault.redeem(got), alice));
    must(big(out.assets) >= 1_000_000_000n * ONE - 1n && (await bal(A)) === a0 + big(out.net), `redeem ${JSON.stringify(out)}`);
    return `fee on the way out ${big(out.fee)}`;
  });
  await check('collect fees pays the admin', async () => {
    const f = big((await info()).fees_accrued);
    must(f > 0n, 'no fees accrued');
    const a0 = await bal(A);
    const got = big(okOf(await send(vault.vault.collectFees(A), alice)));
    must(got === f && (await bal(A)) === a0 + f && big((await info()).fees_accrued) === 0n, 'fees mismatch');
    return `collected ${f}`;
  });

  console.log('\n# redeem and receipts');
  await check('preview matches the actual redeem at zero APY', async () => {
    const p = await vault.vault.previewRedeem(4n * ONE).call();
    const out = okOf(await send(vault.vault.redeem(4n * ONE), bob));
    must(big(out.assets) === big(p.assets) && big(out.fee) === big(p.fee) && big(out.net) === big(p.net), `preview ${JSON.stringify(p)} vs ${JSON.stringify(out)}`);
    must(big(out.fee) === (4n * ONE * 30n) / 10_000n, 'fee is 0.3%');
  });
  await check('redeem more shares than held is rejected', async () => expectErr(send(vault.vault.redeem((await shares(B)) + 1n), bob), /receipt tokens|InsufficientShares/i));
  await check('redeem one share pays one unit with zero fee', async () => {
    const b0 = await bal(B);
    const out = okOf(await send(vault.vault.redeem(1n), bob));
    must(big(out.net) === 1n && big(out.fee) === 0n, `out ${JSON.stringify(out)}`);
    must((await bal(B)) === b0 + 1n, 'paid');
  });
  await check('kUSDC transfer to another account, receiver can redeem', async () => {
    okOf(await send(vault.vft.transfer(A, 2n * ONE), bob, G30));
    const a0 = await bal(A);
    const out = okOf(await send(vault.vault.redeem(2n * ONE), alice));
    must((await bal(A)) === a0 + big(out.net), 'receiver paid');
  });
  await check('kUSDC transfer above balance is rejected', async () => expectErr(send(vault.vft.transfer(A, (await shares(B)) + 1n), bob, G30), /./));
  await check('two redeems in flight: the second cannot double spend', async () => {
    const s = await shares(B);
    must(s >= 2n * ONE, `bob has ${s} shares`);
    const b0 = await bal(B);
    // Redeem everything twice; sign both with consecutive nonces without waiting for the first.
    const nonce = (await api.rpc.system.accountNextIndex(bob.address)).toNumber();
    const t1 = vault.vault.redeem(s).withAccount(bob, { nonce }).withGas(G100).signAndSend();
    const t2 = vault.vault.redeem(s).withAccount(bob, { nonce: nonce + 1 }).withGas(G100).signAndSend();
    const [r1, r2] = await Promise.all([t1.then((x) => x.response()), t2.then((x) => x.response())]);
    const oks = [r1, r2].filter((r) => !errOf(r)).length;
    const errs = [r1, r2].map(errOf).filter(Boolean);
    must(oks === 1 && errs.length === 1 && /receipt tokens|InsufficientShares/i.test(errs[0]!), `results ${JSON.stringify([r1, r2], (_, v) => (typeof v === 'bigint' ? v.toString() : v))}`);
    const net = big((oks && !errOf(r1) ? (r1 as { ok: { net: unknown } }).ok : (r2 as { ok: { net: unknown } }).ok).net);
    must((await bal(B)) === b0 + net && (await shares(B)) === 0n, 'paid exactly once');
    return `one succeeded, one rejected: ${errs[0]}`;
  });

  console.log('\n# yield and the mint fallback');
  await check('APY 1000% accrues within seconds and info projects it', async () => {
    // zero fee here so every payout above principal needs the shortfall mint
    okOf(await send(vault.vault.setConfig(100_000, 0, 30), alice, G40));
    const r0 = big(await vault.vault.rate().call());
    await sleep(9_000);
    const r1 = big(await vault.vault.rate().call());
    must(r1 > r0, `rate did not move ${r0} -> ${r1}`);
    return `rate ${r0} -> ${r1}`;
  });
  await check('redeem with yield mints exactly the shortfall', async () => {
    // A 10M USDC position so a few seconds of yield dwarf any fee dust in holdings; forces the mint path.
    okOf(await send(token.admin.mint(A, 10_000_000n * ONE), alice, G30));
    okOf(await send(vault.vault.deposit(10_000_000n * ONE), alice));
    await sleep(6_000);
    const supply0 = big(await token.vft.totalSupply().call());
    const h0 = big((await info()).holdings);
    const out = okOf(await send(vault.vault.redeem(await shares(A)), alice));
    must(big(out.net) > 10_000_000n * ONE, `no yield paid: ${big(out.net)}`);
    const supply1 = big(await token.vft.totalSupply().call());
    const minted = supply1 - supply0;
    const expected = big(out.net) > h0 ? big(out.net) - h0 : 0n;
    must(minted === expected && minted > 0n, `minted ${minted} expected ${expected} (holdings before ${h0})`);
    return `paid ${big(out.net)}, minted ${minted}`;
  });
  await check('redeem when the vault lost its minter role fails and restores shares', async () => {
    okOf(await send(token.admin.revokeMinter(V), alice, G30));
    okOf(await send(vault.vault.deposit(10_000_000n * ONE), alice));
    await sleep(6_000);
    const s0 = await shares(A); const b0 = await bal(A); const fees0 = big((await info()).fees_accrued);
    try {
      const m = await expectErr(send(vault.vault.redeem(s0), alice), /token rejected|Unauthorized/i);
      must((await shares(A)) === s0, 'shares not restored');
      must((await bal(A)) === b0, 'balance changed');
      must(big((await info()).fees_accrued) === fees0, 'fees not restored');
      return m;
    } finally {
      okOf(await send(token.admin.grantMinter(V), alice, G30));
    }
  });
  await check('after the role is back the same redeem succeeds', async () => {
    const s0 = await shares(A);
    const b0 = await bal(A);
    const out = okOf(await send(vault.vault.redeem(s0), alice));
    must((await shares(A)) === 0n && (await bal(A)) === b0 + big(out.net), 'payout');
    okOf(await send(vault.vault.setConfig(100_000, 30, 30), alice, G40));
  });
  await check('accrue is callable by anyone', async () => {
    const r = big(await send(vault.vault.accrue(), bob, G40));
    must(r > SCALE, `rate ${r}`);
  });

  console.log('\n# unbonding');
  let entryId = 0;
  let entryAssets = 0n;
  await check('request unbond locks assets at the current rate', async () => {
    okOf(await send(vault.vault.deposit(50n * ONE), bob));
    const e = okOf(await send(vault.vault.requestUnbond(20n * ONE), bob, G40));
    entryId = Number(e.id); entryAssets = big(e.assets);
    must(entryAssets >= 20n * ONE, `assets ${entryAssets}`);
    must(Number(e.claimable_at) - Number(e.requested_at) === 30_000, 'period');
    const list = await vault.vault.unbonds(B).call();
    must(list.length === 1 && Number(list[0].id) === entryId, 'entry listed');
  });
  await check('locked assets do not keep accruing', async () => {
    await sleep(4_000);
    const list = await vault.vault.unbonds(B).call();
    must(big(list[0].assets) === entryAssets, 'assets changed');
  });
  await check('request unbond above balance is rejected', async () => expectErr(send(vault.vault.requestUnbond((await shares(B)) + 1n), bob, G40), /receipt tokens|InsufficientShares/i));
  await check('claim before maturity is rejected', async () => expectErr(send(vault.vault.claim(entryId), bob), /not ended|UnbondNotReady/i));
  await check('claim by a different account is rejected', async () => expectErr(send(vault.vault.claim(entryId), alice), /no longer exists|UnbondNotFound/i));
  await check('claim after maturity pays, second claim is rejected', async () => {
    await sleep(31_000);
    const b0 = await bal(B);
    const got = big(okOf(await send(vault.vault.claim(entryId), bob)));
    must(got === entryAssets && (await bal(B)) === b0 + got, 'payout');
    await expectErr(send(vault.vault.claim(entryId), bob), /no longer exists|UnbondNotFound/i);
    must((await vault.vault.unbonds(B).call()).length === 0, 'entry still listed');
  });

  console.log('\n# pause and admin');
  await check('non-admin cannot change config, pause, or transfer admin', async () => {
    await expectErr(send(vault.vault.setConfig(1, 1, 1), bob, G40), /admin|Unauthorized/i);
    await expectErr(send(vault.vault.pause(), bob, G40), /admin|Unauthorized/i);
    await expectErr(send(vault.vault.transferAdmin(B), bob, G40), /admin|Unauthorized/i);
    await expectErr(send(vault.vault.collectFees(B), bob), /admin|Unauthorized/i);
  });
  await check('config bounds are enforced', async () => {
    await expectErr(send(vault.vault.setConfig(100_001, 30, 30), alice, G40), /BadConfig/i);
    await expectErr(send(vault.vault.setConfig(100, 1_001, 30), alice, G40), /BadConfig/i);
  });
  await check('paused vault rejects deposit, redeem and unbond, then resumes', async () => {
    okOf(await send(vault.vault.pause(), alice, G40));
    await expectErr(send(vault.vault.deposit(5n * ONE), bob), /paused/i);
    await expectErr(send(vault.vault.redeem(1n), bob), /paused/i);
    await expectErr(send(vault.vault.requestUnbond(1n), bob, G40), /paused/i);
    await expectErr(send(vault.vft.transfer(A, 1n), bob, G30), /./);
    okOf(await send(vault.vault.resume(), alice, G40));
    okOf(await send(vault.vault.deposit(5n * ONE), bob));
  });
  await check('paused token makes deposit fail cleanly', async () => {
    okOf(await send(token.admin.pause(), alice, G30));
    const s0 = await shares(B);
    await expectErr(send(vault.vault.deposit(5n * ONE), bob), /paused|rejected/i);
    must((await shares(B)) === s0, 'shares minted');
    okOf(await send(token.admin.resume(), alice, G30));
  });
  await check('top up reserve mints into the vault', async () => {
    const h0 = big((await info()).holdings);
    okOf(await send(vault.vault.topUpReserve(1_000n * ONE), alice));
    must(big((await info()).holdings) === h0 + 1_000n * ONE, 'holdings');
    must((await bal(V)) >= h0 + 1_000n * ONE, 'token balance');
  });
  await check('admin transfer hands control over and back', async () => {
    okOf(await send(vault.vault.transferAdmin(B), alice, G40));
    await expectErr(send(vault.vault.pause(), alice, G40), /admin|Unauthorized/i);
    okOf(await send(vault.vault.transferAdmin(A), bob, G40));
    must((await info()).admin === A, 'admin');
  });
  await check('faucet config: admin can retarget and disable', async () => {
    okOf(await send(token.admin.setFaucet(0n, 30), alice, G30));
    await expectErr(send(token.faucet.claim(), bob, G30), /switched off|FaucetDisabled/i);
    okOf(await send(token.admin.setFaucet(1_000n * ONE, 30), alice, G30));
  });

  console.log('\n# app adapter on top of the same deployment');
  await check('GearAdapter reads stats, balances, unbonds, faucet for a user', async () => {
    const adapter = new GearAdapter('mainnet', { programs: dep.programs, rpc: RPC, signer: async () => ({ account: bob }) });
    const st = await adapter.getStats();
    must(st.vaults[ASSET].rate >= 1_000_000_000n && typeof st.blockNumber === 'number', 'stats');
    const b = await adapter.getBalances(bob.address);
    must(b[ASSET] === (await bal(B)) && b[`k${ASSET}`] === (await shares(B)), 'balances');
    const f = await adapter.getFaucet(bob.address);
    must(f[ASSET].amount === 1_000n * ONE, 'faucet');
    const r = await adapter.depositVault(bob.address, ASSET, 3n * ONE);
    must(r.shares! > 0n, 'deposit via adapter');
    await adapter.dispose();
    return `block ${st.blockNumber}`;
  });

  await api.disconnect();
  const failed = results.filter((r) => !r.ok);
  console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
  if (failed.length) { console.log(failed.map((f) => `  ✗ ${f.name}: ${f.note}`).join('\n')); process.exit(1); }
}

main().catch((e) => { console.error(e); process.exit(1); });
