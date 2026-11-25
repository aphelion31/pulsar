// Widget campaign E2E: a fresh visitor on a matching page gets the campaign message after time_on_page.
// Usage: BASE=... WEBSITE_TOKEN=... INBOX_ID=... TOKEN=<agent api token> node tools/e2e-widget-campaign.js
const { chromium } = require('playwright');
const { BASE, WEBSITE_TOKEN: WT, INBOX_ID, TOKEN } = process.env;
(async () => {
  const h = { api_access_token: TOKEN, 'content-type': 'application/json' };
  const msg = `Campaign hello ${Date.now()}`;
  const camp = await (await fetch(`${BASE}/api/v1/accounts/1/campaigns`, { method: 'POST', headers: h, body: JSON.stringify({ title: 'e2e', message: msg, inbox_id: Number(INBOX_ID), sender_id: 1, enabled: true, trigger_rules: { url: `${BASE}/*`, time_on_page: 1 } }) })).json();
  let ok = false;
  try {
    const b = await chromium.launch();
    const p = await b.newPage();
    await p.goto(`${BASE}/widget?website_token=${WT}`);
    ok = await p.getByText(msg).waitFor({ timeout: 8000 }).then(() => true, () => false);
    await b.close();
  } finally {
    await fetch(`${BASE}/api/v1/accounts/1/campaigns/${camp.id}`, { method: 'DELETE', headers: h });
  }
  console.log(JSON.stringify({ campaign: camp.id, shown: ok }));
  process.exit(ok ? 0 : 1);
})();
