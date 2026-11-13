// IMAP + SMTP settings for email inboxes (mirrors settings/inbox/ImapSettings.vue + SmtpSettings.vue).
import { useState, type FormEvent } from "react";
import Button from "~/components/next/Button";
import { t } from "~/lib/i18n";

type Ch = Record<string, string | number | boolean | null | undefined>;
const input = "rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm";

export default function EmailChannelSettings({ inbox, save }: { inbox: Ch; save: (channel: Ch) => Promise<void> }) {
  const [v, setV] = useState<Ch>({ ...inbox, imap_password: "", smtp_password: "" });
  const [busy, setBusy] = useState<"" | "imap" | "smtp">("");
  const [msg, setMsg] = useState<Record<string, string>>({});
  const set = (k: string, val: Ch[string]) => setV((p) => ({ ...p, [k]: val }));

  async function submit(kind: "imap" | "smtp", e: FormEvent) {
    e.preventDefault();
    setBusy(kind);
    const keys = Object.keys(v).filter((k) => k.startsWith(`${kind}_`));
    try {
      await save(Object.fromEntries(keys.map((k) => [k, k.endsWith("_port") ? Number(v[k]) || 0 : v[k]])));
      setMsg({ [kind]: t(`INBOX_MGMT.${kind.toUpperCase()}.EDIT.SUCCESS_MESSAGE`) });
    } catch {
      setMsg({ [kind]: t(`INBOX_MGMT.${kind.toUpperCase()}.EDIT.ERROR_MESSAGE`) });
    } finally {
      setBusy("");
    }
  }

  const text = (kind: string, field: string, type = "text") => (
    <label className="grid gap-1 text-sm">
      {t(`INBOX_MGMT.${kind.toUpperCase()}.${field.toUpperCase()}.LABEL`)}
      <input className={input} type={type} value={String(v[`${kind}_${field}`] ?? "")} placeholder={t(`INBOX_MGMT.${kind.toUpperCase()}.${field.toUpperCase()}.PLACE_HOLDER`)} onChange={(e) => set(`${kind}_${field}`, e.target.value)} autoComplete={type === "password" ? "new-password" : "off"} />
    </label>
  );
  const check = (key: string, label: string, help?: string) => (
    <label className="flex items-start gap-3 text-sm">
      <input type="checkbox" checked={!!v[key]} onChange={(e) => set(key, e.target.checked)} />
      <span>{label}{help && <span className="block text-n-slate-11">{help}</span>}</span>
    </label>
  );
  const section = (kind: "imap" | "smtp", body: React.ReactNode) => (
    <form onSubmit={(e) => submit(kind, e)} className="grid gap-4 border-t border-n-weak pt-6">
      <div>
        <h2 className="font-medium">{t(`INBOX_MGMT.${kind.toUpperCase()}.TITLE`)}</h2>
        <p className="text-sm text-n-slate-11">{t(`INBOX_MGMT.${kind.toUpperCase()}.SUBTITLE`)}</p>
      </div>
      {check(`${kind}_enabled`, t(`INBOX_MGMT.${kind.toUpperCase()}.TOGGLE_AVAILABILITY`), t(`INBOX_MGMT.${kind.toUpperCase()}.TOGGLE_HELP`))}
      {v[`${kind}_enabled`] && body}
      {msg[kind] && <p role="status" className="text-sm">{msg[kind]}</p>}
      <div><Button type="submit" disabled={!!busy} isLoading={busy === kind} label={t(`INBOX_MGMT.${kind.toUpperCase()}.UPDATE`)} /></div>
    </form>
  );

  return (
    <div className="grid max-w-2xl gap-6">
      {section("imap", <>
        {text("imap", "address")}{text("imap", "port", "number")}{text("imap", "login")}{text("imap", "password", "password")}
        {check("imap_enable_ssl", t("INBOX_MGMT.IMAP.ENABLE_SSL"))}
      </>)}
      {section("smtp", <>
        {text("smtp", "address")}{text("smtp", "port", "number")}{text("smtp", "login")}{text("smtp", "password", "password")}{text("smtp", "domain")}
        <fieldset className="grid gap-2 text-sm">
          <legend className="mb-1">{t("INBOX_MGMT.SMTP.ENCRYPTION")}</legend>
          <label className="flex items-center gap-2"><input type="radio" name="smtp_enc" checked={!!v.smtp_enable_ssl_tls} onChange={() => setV((p) => ({ ...p, smtp_enable_ssl_tls: true, smtp_enable_starttls_auto: false }))} />{t("INBOX_MGMT.SMTP.SSL_TLS")}</label>
          <label className="flex items-center gap-2"><input type="radio" name="smtp_enc" checked={!v.smtp_enable_ssl_tls && v.smtp_enable_starttls_auto !== false} onChange={() => setV((p) => ({ ...p, smtp_enable_ssl_tls: false, smtp_enable_starttls_auto: true }))} />{t("INBOX_MGMT.SMTP.START_TLS")}</label>
        </fieldset>
      </>)}
    </div>
  );
}
