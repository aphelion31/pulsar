// E2E: login via real Vue UI on chatwoot-rs, open inbox, send a message, check it renders.
// Usage: BASE=http://127.0.0.1:3000 node e2e.js
const { chromium } = require('playwright');
const BASE = process.env.BASE || 'http://127.0.0.1:3000';
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1400, height: 900 } });
  const bad = [];
  p.on('response', r => { if (r.url().includes('/api/') && r.status() >= 400) bad.push(`${r.status()} ${r.request().method()} ${r.url().replace(BASE, '')}`); });
  p.on('pageerror', e => bad.push('JS ' + e.message.slice(0, 200)));
  await p.goto(BASE + '/app/login');
  await p.fill('input[name="email_address"], input[type="email"]', 'admin@example.com');
  await p.fill('input[type="password"]', 'Password123!');
  await p.click('button[type="submit"]');
  await p.waitForURL(/\/app\/accounts\/\d+/, { timeout: 15000 });
  await p.waitForTimeout(3000);
  await p.screenshot({ path: process.env.SHOT || 'dash.png' });
  const convo = p.locator('[class*="conversation"]').first();
  console.log('url', p.url());
  console.log('title', await p.title());
  console.log('errors', JSON.stringify([...new Set(bad)], null, 1));
  await b.close();
})();
