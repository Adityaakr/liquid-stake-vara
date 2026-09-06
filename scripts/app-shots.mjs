// Regenerates the dashboard screenshots the landing/product pages embed (public/fx/app-*.jpg).
// Desktop captures at 1400px and phone captures at 390px, both at 2x for crisp rendering.
// Usage: pnpm build && node scripts/app-shots.mjs
import { chromium } from 'playwright';
import { spawn } from 'node:child_process';
const root = new URL('..', import.meta.url).pathname;
const srv = spawn('node', ['node_modules/vite/bin/vite.js', 'preview', '--port', '4175', '--strictPort'], { cwd: root, stdio: 'ignore' });
await new Promise((r) => setTimeout(r, 2500));
const browser = await chromium.launch();
async function session(viewport) {
  const ctx = await browser.newContext({ viewport, deviceScaleFactor: 2 });
  // Sign in as the simulation's demo account before the app loads; the sidebar toggle is hidden on phones.
  await ctx.addInitScript(() => localStorage.setItem('vale.wallet.v1', JSON.stringify({ address: 'kGj1akEAemmGoVyFqeHVSNUyUj1mJp3gazUYy7Zs2p7BsgT88', name: 'Demo', source: 'demo' })));
  const page = await ctx.newPage();
  await page.goto('http://localhost:4175/app', { waitUntil: 'load' }); await page.waitForTimeout(600);
  await page.getByRole('textbox', { name: 'You stake' }).fill('400');
  await page.getByRole('button', { name: 'Stake' }).click(); await page.waitForTimeout(1200);
  await page.evaluate(() => document.querySelectorAll('[aria-label="Dismiss"]').forEach((b) => b.click()));
  await page.getByRole('textbox', { name: 'You stake' }).fill('250'); await page.waitForTimeout(300);
  return { ctx, page };
}
async function shot(page, path, out, h) {
  if (path) { await page.goto('http://localhost:4175' + path, { waitUntil: 'load' }); await page.waitForTimeout(700); }
  await page.evaluate(() => document.querySelectorAll('[aria-label="Dismiss"]').forEach((b) => b.click()));
  await page.waitForTimeout(200);
  const w = page.viewportSize().width;
  await page.screenshot({ path: `${root}public/fx/${out}`, type: 'jpeg', quality: 90, clip: { x: 0, y: 0, width: w, height: h } });
  console.log('wrote', out);
}
try {
  const d = await session({ width: 1400, height: 860 });
  await shot(d.page, null, 'app-hero.jpg', 846);
  await shot(d.page, '/app/vaults', 'app-overview.jpg', 846);
  await shot(d.page, '/app/portfolio', 'app-portfolio.jpg', 800);
  await d.ctx.close();
  const m = await session({ width: 390, height: 844 });
  await shot(m.page, null, 'app-hero-m.jpg', 760);
  await shot(m.page, '/app/vaults', 'app-overview-m.jpg', 760);
  await shot(m.page, '/app/portfolio', 'app-portfolio-m.jpg', 700);
  await m.ctx.close();
} finally { await browser.close(); srv.kill(); }
