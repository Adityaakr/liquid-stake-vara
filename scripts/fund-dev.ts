// Fund dev accounts from //Alice on a local node: tsx scripts/fund-dev.ts //Charlie //Dave
import { GearApi, GearKeyring } from '@gear-js/api';
async function main() {
  const api = await GearApi.create({ providerAddress: process.env.RPC ?? 'ws://127.0.0.1:9944' });
  const alice = await GearKeyring.fromSuri('//Alice');
  for (const who of process.argv.slice(2)) {
    const p = await GearKeyring.fromSuri(who);
    await new Promise<void>((res, rej) => { api.balance.transfer(p.address, 200n * 10n ** 12n).signAndSend(alice, ({ status, dispatchError }) => { if (dispatchError) rej(new Error(String(dispatchError))); if (status.isInBlock) res(); }).catch(rej); });
    console.log(who, 'funded');
  }
  await api.disconnect();
}
main().catch((e) => { console.error(e); process.exit(1); });
