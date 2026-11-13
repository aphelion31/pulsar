import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { ErrorText, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../../shell";

// mirrors chatwoot/app/javascript/dashboard/routes/dashboard/settings/security/Index.vue and EnforceMfa.vue
// Read-only: Pulsar has no MFA enforcement backend, so the enforce toggle is not wired.
export function SecuritySettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [enforce, setEnforce] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  useEffect(() => {
    api<{ settings?: { enforce_mfa?: boolean } }>(acct(accountId))
      .then(a => setEnforce(a.settings?.enforce_mfa === true))
      .catch(e => setError(e instanceof Error ? e.message : "Request failed"))
      .finally(() => setLoading(false));
  }, [accountId]);

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={t("SECURITY_SETTINGS.TITLE")} />
      <div className="flex-1 overflow-auto p-6">
        <p className="mb-6 max-w-2xl text-sm text-ink-3">{t("SECURITY_SETTINGS.DESCRIPTION")}</p>
        <ErrorText>{error}</ErrorText>
        {!loading && (
          <div className="flex items-center justify-between gap-4 rounded-lg border border-line p-4">
            <div>
              <p className="text-sm font-medium">{t("MFA_SETTINGS.TITLE")}</p>
              <p className="mt-1 text-sm text-ink-3">{t("MFA_SETTINGS.DESCRIPTION")}</p>
            </div>
            <span className="text-sm text-ink-2">{enforce ? t("MFA_SETTINGS.ENABLED") : t("MFA_SETTINGS.DISABLED")}</span>
          </div>
        )}
      </div>
    </section>
  );
}

// mirrors chatwoot/app/javascript/dashboard/routes/dashboard/settings/conversationWorkflow/index.vue
// Auto-resolve fields live in account settings (scheduler.rs reads the same keys).
export function ConversationWorkflow() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [duration, setDuration] = useState("");
  const [message, setMessage] = useState("");
  const [ignoreWaiting, setIgnoreWaiting] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api<{ settings?: Record<string, unknown> }>(acct(accountId))
      .then(a => {
        const s = a.settings ?? {};
        setDuration(s.auto_resolve_after == null ? "" : String(s.auto_resolve_after));
        setMessage(String(s.auto_resolve_message ?? ""));
        setIgnoreWaiting(s.auto_resolve_ignore_waiting === true);
      })
      .catch(e => setError(e instanceof Error ? e.message : "Request failed"))
      .finally(() => setLoading(false));
  }, [accountId]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    const minutes = Number(duration);
    if (duration !== "" && (!Number.isInteger(minutes) || minutes < 10 || minutes > 999 * 24 * 60)) {
      setError(t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.ERROR", { minDuration: 10 }));
      return;
    }
    setSaving(true); setError(""); setSaved(false);
    try {
      await api(acct(accountId), {
        method: "PATCH",
        json: {
          settings: {
            auto_resolve_after: duration === "" ? null : minutes,
            auto_resolve_message: message,
            auto_resolve_ignore_waiting: ignoreWaiting,
          },
        },
      });
      setSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.API.ERROR"));
    } finally { setSaving(false); }
  }

  const input = "w-full rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm";
  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={t("CONVERSATION_WORKFLOW.INDEX.HEADER.TITLE")} />
      <div className="flex-1 overflow-auto p-6">
        <p className="mb-6 max-w-2xl text-sm text-ink-3">{t("CONVERSATION_WORKFLOW.INDEX.HEADER.DESCRIPTION")}</p>
        {loading ? null : (
          <form onSubmit={save} className="grid max-w-2xl gap-5 rounded-lg border border-line p-5">
            <h2 className="text-sm font-medium">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.TITLE")}</h2>
            <p className="text-sm text-ink-3">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.NOTE")}</p>
            <label className="grid gap-1 text-sm">
              {t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.LABEL")}
              <input className={input} type="number" min="0" value={duration} placeholder={t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.PLACEHOLDER")} onChange={e => setDuration(e.target.value)} />
              <span className="text-ink-3">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.HELP")}</span>
            </label>
            <label className="grid gap-1 text-sm">
              {t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.MESSAGE.LABEL")}
              <textarea className={input} rows={3} value={message} placeholder={t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.MESSAGE.PLACEHOLDER")} onChange={e => setMessage(e.target.value)} />
              <span className="text-ink-3">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.MESSAGE.HELP")}</span>
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={ignoreWaiting} onChange={e => setIgnoreWaiting(e.target.checked)} />
              {t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.IGNORE_WAITING.LABEL")}
            </label>
            <ErrorText>{error}</ErrorText>
            {saved && <p className="text-sm text-ink-2">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.API.SUCCESS")}</p>}
            <div>
              <button type="submit" disabled={saving} className="rounded-lg bg-n-brand px-4 py-2 text-sm text-white disabled:opacity-50">
                {t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.UPDATE_BUTTON")}
              </button>
            </div>
          </form>
        )}
      </div>
    </section>
  );
}

// mirrors chatwoot/app/javascript/dashboard/routes/dashboard/profile/mfa
// Pulsar has no MFA backend yet: status is shown read-only as disabled and the enable flow is not ported.
export function ProfileMfa() {
  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={t("MFA_SETTINGS.TITLE")} />
      <div className="flex-1 overflow-auto p-6">
        <p className="mb-6 max-w-2xl text-sm text-ink-3">{t("MFA_SETTINGS.SUBTITLE")}</p>
        <div className="rounded-lg border border-line p-4">
          <p className="text-sm font-medium">{t("MFA_SETTINGS.STATUS_TITLE")}</p>
          <p className="mt-1 text-sm text-ink-3">{t("MFA_SETTINGS.STATUS_DESCRIPTION")}</p>
          <p className="mt-3 text-sm text-ink-2">{t("MFA_SETTINGS.DISABLED")}</p>
        </div>
      </div>
    </section>
  );
}
