// CSAT in the widget: agent resolves, the survey appears live, the visitor taps an emoji, rating is stored.
// Usage: BASE=... WEBSITE_TOKEN=... INBOX_ID=... TOKEN=<agent api token> node tools/e2e-widget-csat.js
const { chromium } = require('playwright');
const { execSync } = require('child_process');
const { BASE, WEBSITE_TOKEN: WT, INBOX_ID, TOKEN } = process.env;
const sql = (q) => execSync(`psql -qtAX postgres://localhost/pulsar -c "${q}"`).toString().trim();
(async () => {
  const before = sql(`select coalesce(csat_survey_enabled::text,'null') from inboxes where id=${INBOX_ID}`);
  sql(`update inboxes set csat_survey_enabled=true where id=${INBOX_ID}`);
  let ok = false, conv;
  try {
    const b = await chromium.launch();
    const p = await b.newPage();
    await p.goto(`${BASE}/widget?website_token=${WT}`);
    const text = `csat e2e ${Date.now()}`;
    await p.getByLabel('Message', { exact: true }).fill(text);
    await p.getByRole('button', { name: 'Send message' }).click();
    await p.getByText(text).waitFor({ timeout: 5000 });
    conv = sql(`select c.display_id from conversations c join messages m on m.conversation_id=c.id where m.content='${text}'`);
    await fetch(`${BASE}/api/v1/accounts/1/conversations/${conv}/toggle_status`, { method: 'POST', headers: { api_access_token: TOKEN, 'content-type': 'application/json' }, body: JSON.stringify({ status: 'resolved' }) });
    await p.getByRole('button', { name: 'Good' }).click({ timeout: 8000 });
    await p.getByText('Thank you for submitting the rating').waitFor({ timeout: 5000 });
    await p.waitForTimeout(1500);
    ok = sql(`select rating from csat_survey_responses r join conversations c on c.id=r.conversation_id where c.account_id=1 and c.display_id=${conv}`) === '4';
    await b.close();
  } finally {
    sql(`update inboxes set csat_survey_enabled=${before} where id=${INBOX_ID}`);
    if (conv) sql(`delete from csat_survey_responses where conversation_id=(select id from conversations where account_id=1 and display_id=${conv})`);
  }
  console.log(JSON.stringify({ conv, stored: ok }));
  process.exit(ok ? 0 : 1);
})();
