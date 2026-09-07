// Does a vault with no extra VARA funding still complete deposit/redeem? (Gas comes from the caller.)
import { readFileSync } from 'node:fs';
import { GearApi, GearKeyring, decodeAddress } from '@gear-js/api';
import { DemoToken } from '../src/chain/idl/demo_token';
import { Vault } from '../src/chain/idl/vault';

async function main() {
  const dep = JSON.parse(readFileSync('deployments/local.json', 'utf8'));
  const api = await GearApi.create({ providerAddress: 'ws://127.0.0.1:9944' });
  const alice = await GearKeyring.fromSuri('//Alice');
  const dave = await GearKeyring.fromSuri('//Dave');
  const wasm = readFileSync('programs/vault/target/wasm32-gear/release/vault.opt.wasm');
  const token = new DemoToken(api, dep.programs.USDC.token);
  const vault = new Vault(api);
  const t = await vault.newCtorFromCode(wasm, dep.programs.USDC.token, 'zero fund', 'zUSDC', 6, 0, 30, 3_600, 0n).withAccount(alice).withGas(150_000_000_000n).signAndSend();
  await t.response();
  console.log('vault', vault.programId, 'balance', (await api.balance.findOut(vault.programId)).toString());
  await (await token.faucet.claim().withAccount(dave).withGas(30_000_000_000n).signAndSend()).response();
  await (await token.vft.approve(vault.programId, 2n ** 256n - 1n).withAccount(dave).withGas(30_000_000_000n).signAndSend()).response();
  const d = await (await vault.vault.deposit(500_000_000n).withAccount(dave).withGas(100_000_000_000n).signAndSend()).response();
  console.log('deposit', JSON.stringify(d));
  await new Promise((r) => setTimeout(r, 6000));
  const r = await (await vault.vault.redeem(BigInt(String(await vault.vft.balanceOf(decodeAddress(dave.address)).call()))).withAccount(dave).withGas(100_000_000_000n).signAndSend()).response();
  console.log('redeem', JSON.stringify(r));
  console.log('vault balance after', (await api.balance.findOut(vault.programId)).toString());
  await api.disconnect();
}
main().catch((e) => { console.error(e); process.exit(1); });
