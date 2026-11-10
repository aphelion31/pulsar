// E2E for the React Router frontend served by Pulsar: sign in, open a conversation, send a reply,
// then create an incoming message via API and check it appears live over the websocket.
// Usage: BASE=http://127.0.0.1:3000 node tools/e2e.js   (needs `npm i playwright` and one conversation, id 1)
const { chromium } = require('playwright');
const BASE = process.env.BASE || 'http://127.0.0.1:3000';
const SHOT = process.env.SHOT || 'e2e.png';
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
  const bad = [];
  p.on('response', (r) => { if (r.url().includes('/api/') && r.status() >= 400) bad.push(`${r.status()} ${r.request().method()} ${r.url().replace(BASE, '')}`); });
  p.on('pageerror', (e) => bad.push('JS ' + e.message.slice(0, 200)));

  await p.goto(BASE + '/app/login');
  await p.getByLabel('Email').fill('admin@example.com');
  await p.getByLabel('Password').fill(process.env.PASSWORD || 'Password123!');
  await p.getByRole('button', { name: 'Sign in' }).click();
  await p.waitForURL(/\/app\/accounts\/\d+\/conversations/, { timeout: 15000 });

  await p.goto(BASE + '/app/accounts/1/conversations/1');
  const reply = `e2e reply ${Date.now()}`;
  await p.getByRole('textbox', { name: 'Reply' }).fill(reply);
  await p.getByRole('button', { name: 'Send' }).click();
  await p.getByText(reply).first().waitFor({ timeout: 5000 });
  const sent = (await p.getByRole('textbox', { name: 'Reply' }).inputValue()) === '' ? 1 : 0;

  const live = `live via ws ${Date.now()}`;
  await p.evaluate(async (live) => {
    const s = JSON.parse(localStorage.getItem('pulsar.session'));
    await fetch('/api/v1/accounts/1/conversations/1/messages', {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'access-token': s['access-token'], client: s.client, uid: s.uid },
      body: JSON.stringify({ content: live, message_type: 'incoming' }),
    });
  }, live);
  const realtime = await p.getByText(live).first().waitFor({ timeout: 5000 }).then(() => 1, () => 0);
  await p.screenshot({ path: SHOT });
  console.log(JSON.stringify({ url: p.url(), sent, realtime, errors: [...new Set(bad)] }, null, 1));
  await b.close();
  process.exit(sent && realtime ? 0 : 1);
})();
