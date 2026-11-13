// Active contacts: a visitor with an open widget shows up in GET /contacts/active, and only then.
// Usage: BASE=http://127.0.0.1:3110 WEBSITE_TOKEN=... node tools/active-contacts.js
const { chromium } = require('playwright');
const BASE = process.env.BASE || 'http://127.0.0.1:3000';
const WT = process.env.WEBSITE_TOKEN;
(async () => {
  const login = await fetch(`${BASE}/auth/sign_in`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: 'admin@example.com', password: process.env.PASSWORD || 'Password123!' }) });
  const h = { 'access-token': login.headers.get('access-token'), client: login.headers.get('client'), uid: login.headers.get('uid') };
  const active = async () => (await (await fetch(`${BASE}/api/v1/accounts/1/contacts/active`, { headers: h })).json()).payload.map((c) => c.id);
  const before = await active();

  const b = await chromium.launch();
  const p = await b.newPage();
  await p.goto(`${BASE}/widget?website_token=${WT}`);
  const text = `active check ${Date.now()}`;
  await p.getByLabel('Message', { exact: true }).fill(text);
  await p.getByRole('button', { name: 'Send message' }).click();
  await p.getByText(text).waitFor({ timeout: 5000 });
  await p.waitForTimeout(1500);
  const list = await (await fetch(`${BASE}/api/v1/accounts/1/conversations?status=all&assignee_type=all`, { headers: h })).json();
  const contactId = list.data.payload.find((c) => c.last_non_activity_message?.content === text)?.meta.sender.id;
  const during = await active();
  await b.close();

  const ok = contactId && !before.includes(contactId) && during.includes(contactId);
  console.log(JSON.stringify({ contactId, before: before.length, during: during.length, ok }));
  process.exit(ok ? 0 : 1);
})();
