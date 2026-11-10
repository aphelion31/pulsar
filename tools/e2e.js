// E2E: login via the real Vue UI on chatwoot-rs, open a conversation, send a reply, verify it renders,
// then inject a message via API and verify it arrives over the websocket without reload.
// Usage: BASE=http://127.0.0.1:3000 node tools/e2e.js   (needs `npm i playwright` and one existing conversation)
const { chromium } = require('playwright');
const BASE = process.env.BASE || 'http://127.0.0.1:3000';
const SHOT = process.env.SHOT || 'e2e.png';
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1400, height: 900 } });
  const bad = [];
  p.on('response', r => { if (r.url().includes('/api/') && r.status() >= 400) bad.push(`${r.status()} ${r.request().method()} ${r.url().replace(BASE, '')}`); });
  p.on('pageerror', e => bad.push('JS ' + e.message.slice(0, 200)));
  await p.goto(BASE + '/app/login');
  await p.fill('input[type="email"], input[name="email_address"]', 'admin@example.com');
  await p.fill('input[type="password"]', 'Password123!');
  await p.click('button[type="submit"]');
  await p.waitForURL(/\/app\/accounts\/\d+/, { timeout: 15000 });

  // open first conversation in "All"
  await p.goto(BASE + '/app/accounts/1/conversations/1');
  await p.waitForTimeout(2500);
  const reply = `e2e reply ${Date.now()}`;
  const editor = p.locator('.ProseMirror').first();
  await editor.click();
  await editor.type(reply);
  await p.getByRole('button', { name: /^Send/ }).click();
  await p.waitForTimeout(2000);
  // after send, editor is cleared, so any remaining match is the rendered bubble
  const sent = (await editor.innerText()).includes(reply) ? 0 : await p.getByText(reply).count();

  // realtime: message created via API must appear without reload
  const auth = JSON.parse(decodeURIComponent((await p.context().cookies()).find(c => c.name === 'cw_d_session_info').value));
  const live = `live via ws ${Date.now()}`;
  await p.evaluate(async ({ live, auth }) => {
    await fetch('/api/v1/accounts/1/conversations/1/messages', {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'access-token': auth['access-token'], client: auth.client, uid: auth.uid },
      body: JSON.stringify({ content: live, message_type: 'incoming' }),
    });
  }, { live, auth });
  await p.waitForTimeout(2000);
  const realtime = await p.getByText(live).count();
  await p.screenshot({ path: SHOT });
  console.log(JSON.stringify({ url: p.url(), sent, realtime, errors: [...new Set(bad)] }, null, 1));
  await b.close();
  process.exit(sent && realtime ? 0 : 1);
})();
