// E2E for the public website widget: a visitor sends a message, an agent replies through the API,
// and the reply shows up live in the widget. Usage: BASE=... WEBSITE_TOKEN=... node tools/e2e-widget.js
const { chromium } = require('playwright');
const BASE = process.env.BASE || 'http://127.0.0.1:3000';
const WT = process.env.WEBSITE_TOKEN;
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 400, height: 640 } });
  const bad = [];
  p.on('response', (r) => { if (r.url().includes('/api/') && r.status() >= 400) bad.push(`${r.status()} ${r.url().replace(BASE, '')}`); });
  p.on('pageerror', (e) => bad.push('JS ' + e.message));
  await p.goto(`${BASE}/widget?website_token=${WT}`);
  const text = `visitor says hi ${Date.now()}`;
  await p.getByLabel('Message', { exact: true }).fill(text);
  await p.getByRole('button', { name: 'Send message' }).click();
  await p.getByText(text).waitFor({ timeout: 5000 });

  // agent replies via the dashboard API
  const login = await fetch(`${BASE}/auth/sign_in`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: 'admin@example.com', password: process.env.PASSWORD || 'Password123!' }) });
  const h = { 'access-token': login.headers.get('access-token'), client: login.headers.get('client'), uid: login.headers.get('uid'), 'content-type': 'application/json' };
  const list = await (await fetch(`${BASE}/api/v1/accounts/1/conversations?status=all&assignee_type=all`, { headers: h })).json();
  const conv = list.data.payload.find((c) => c.last_non_activity_message?.content === text);
  const reply = `agent answer ${Date.now()}`;
  await fetch(`${BASE}/api/v1/accounts/1/conversations/${conv.id}/messages`, { method: 'POST', headers: h, body: JSON.stringify({ content: reply }) });
  const secret = `internal note ${Date.now()}`;
  await fetch(`${BASE}/api/v1/accounts/1/conversations/${conv.id}/messages`, { method: 'POST', headers: h, body: JSON.stringify({ content: secret, private: true }) });

  const live = await p.getByText(reply).waitFor({ timeout: 5000 }).then(() => 1, () => 0);
  await p.waitForTimeout(800);
  const leaked = await p.getByText(secret).count();
  await p.screenshot({ path: process.env.SHOT || 'widget.png' });
  console.log(JSON.stringify({ conversation: conv.id, live, privateNoteLeaked: leaked > 0, errors: bad }, null, 1));
  await b.close();
  process.exit(live && !leaked && bad.length === 0 ? 0 : 1);
})();
