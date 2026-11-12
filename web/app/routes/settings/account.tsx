import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type AccountSettings = { id: number; name: string; locale: string; domain: string | null; support_email: string | null; features?: Record<string, boolean> };
const input = "w-full rounded-lg border border-n-weak bg-n-background px-3 py-2";

export default function AccountSettingsPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [account, setAccount] = useState<AccountSettings | null>(null);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let active = true;
    setAccount(null);
    api<AccountSettings>(acct(accountId)).then(value => { if (active) setAccount(value); }).catch(e => { if (active) setError(e.message); });
    return () => { active = false; };
  }, [accountId]);
  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (!account) return;
    setBusy(true); setError(""); setMessage("");
    try {
      const updated = await api<AccountSettings>(acct(accountId), { method: "PATCH", json: { name: account.name, domain: account.domain, support_email: account.support_email } });
      setAccount(updated); setMessage(t("GENERAL_SETTINGS.UPDATE.SUCCESS"));
    } catch (e) { setError(e instanceof Error ? e.message : t("GENERAL_SETTINGS.UPDATE.ERROR")); }
    finally { setBusy(false); }
  }
  return <section className="flex-1 overflow-auto p-8 text-n-slate-12"><div className="max-w-2xl"><h1 className="text-xl font-medium">{t("GENERAL_SETTINGS.TITLE")}</h1><h2 className="mt-6 text-lg font-medium">{t("GENERAL_SETTINGS.FORM.GENERAL_SECTION.TITLE")}</h2>
    {error && <p role="alert" className="mt-4 text-n-ruby-11">{error}</p>}{message && <p role="status" className="mt-4">{message}</p>}
    {account && <form className="mt-4 grid gap-4" onSubmit={save}><label className="grid gap-1 text-sm">{t("GENERAL_SETTINGS.FORM.NAME.LABEL")}<input className={input} required value={account.name} placeholder={t("GENERAL_SETTINGS.FORM.NAME.PLACEHOLDER")} onChange={e => setAccount({ ...account, name: e.target.value })} /></label>
      <label className="grid gap-1 text-sm">{t("GENERAL_SETTINGS.FORM.LANGUAGE.LABEL")}<input className={input} value={account.locale} readOnly /></label>
      {account.features?.inbound_emails && account.features?.custom_reply_domain && <label className="grid gap-1 text-sm">{t("GENERAL_SETTINGS.FORM.DOMAIN.LABEL")}<input className={input} value={account.domain ?? ""} placeholder={t("GENERAL_SETTINGS.FORM.DOMAIN.PLACEHOLDER")} onChange={e => setAccount({ ...account, domain: e.target.value })} /></label>}
      {account.features?.inbound_emails && account.features?.custom_reply_email && <label className="grid gap-1 text-sm">{t("GENERAL_SETTINGS.FORM.SUPPORT_EMAIL.LABEL")}<input className={input} type="email" value={account.support_email ?? ""} placeholder={t("GENERAL_SETTINGS.FORM.SUPPORT_EMAIL.PLACEHOLDER")} onChange={e => setAccount({ ...account, support_email: e.target.value })} /></label>}
      <div><Button type="submit" disabled={busy} label={t("GENERAL_SETTINGS.SUBMIT")} /></div></form>}
    <div className="mt-8 border-t border-n-weak pt-6"><h2 className="text-lg font-medium">{t("GENERAL_SETTINGS.FORM.ACCOUNT_ID.TITLE")}</h2><p className="mt-1 text-sm text-n-slate-11">{t("GENERAL_SETTINGS.FORM.ACCOUNT_ID.NOTE")}</p><p className="mt-2">{accountId}</p></div>
  </div></section>;
}
