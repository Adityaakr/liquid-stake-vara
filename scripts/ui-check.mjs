// Build the app in gear mode against a deployment, serve it, and check the vault screens
// render live on-chain data in a real browser (no wallet extension needed for reads).
//
//   node scripts/ui-check.mjs [--rpc ws://127.0.0.1:9944] [--deployment deployments/local.json]
import { readFileSync } from 'node:fs';
import { spawn, execSync } from 'node:child_process';
import { chromium } from 'playwright';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] ?? 'true'] : [])).filter((x) => x.length));
const RPC = args.rpc ?? 'ws://127.0.0.1:9944';
const dep = JSON.parse(readFileSync(args.deployment ?? 'deployments/local.json', 'utf8'));
const PORT = 4176;

const env = {
  ...process.env,
  VITE_ADAPTER: 'gear',
  VITE_VARA_RPC: RPC,
  VITE_USDC_TOKEN: dep.programs.USDC.token,
  VITE_KUSDC_VAULT: dep.programs.USDC.vault,
  VITE_USDT_TOKEN: dep.programs.USDT.token,
  VITE_KUSDT_VAULT: dep.programs.USDT.vault,
};
execSync('node node_modules/vite/bin/vite.js build --logLevel error', { env, stdio: 'inherit' });
const server = spawn('node', ['node_modules/vite/bin/vite.js', 'preview', '--port', String(PORT), '--strictPort'], { env, stdio: 'ignore' });
await new Promise((r) => setTimeout(r, 1500));

const failures = [];
const expect = (cond, msg) => { if (cond) console.log(`  ✓ ${msg}`); else { failures.push(msg); console.log(`  ✗ ${msg}`); } };
try {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(`http://localhost:${PORT}/app/vaults`);
  await page.waitForFunction(() => /block [\d,]+/.test(document.body.innerText) && document.body.innerText.includes('live'), null, { timeout: 30000 }).catch(() => {});
  const text = await page.innerText('body');
  expect(!text.includes('not configured'), 'vaults are configured (no warning banner)');
  expect((text.match(/\blive\b/g) ?? []).length >= 2, 'both vault cards show the live badge from chain');
  expect(/(Rate|Share price)\s*\n?\s*1\.0\d{3}/.test(text), 'rate is read from the vault program');
  expect(/7\.9%/.test(text) && /8\.4%/.test(text), 'APY comes from the program config');
  expect(/block [\d,]+/.test(text), 'network card shows the latest block');
  expect(!text.includes('simulation'), 'no simulation badge in gear mode');
  await page.screenshot({ path: 'screenshots/ui-check-vaults.png', fullPage: true });
  await page.goto(`http://localhost:${PORT}/app`);
  await page.waitForTimeout(1500);
  const stake = await page.innerText('body');
  expect(stake.includes('You stake') && /\bUSDT\b/.test(stake) && !/Asset: VARA/.test(stake), 'stake page starts on a live stable pool (VARA staking not offered on chain)');
  await page.goto(`http://localhost:${PORT}/app/settings`);
  await page.waitForTimeout(1200);
  const settings = await page.innerText('body');
  expect(settings.includes('Demo tokens') && settings.includes('Connect a wallet to claim'), 'settings page explains the demo token faucet');
  await page.goto(`http://localhost:${PORT}/app/portfolio`);
  await page.waitForTimeout(1000);
  expect((await page.innerText('body')).includes('Connect a wallet to see your positions'), 'portfolio asks for a wallet');
  expect(errors.length === 0, `no page errors${errors.length ? `: ${errors.join(' | ')}` : ''}`);
  await browser.close();
} finally {
  server.kill();
}
console.log(failures.length ? `\n${failures.length} UI checks failed` : '\nall UI checks passed');
process.exit(failures.length ? 1 : 0);
