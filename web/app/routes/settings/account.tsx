import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type AccountSettings = {
  id: number;
  name: string;
  locale: string;
  domain: string | null;
  support_email: string | null;
  features?: Record<string, boolean>;
};

const inputCls =
  "h-10 w-full rounded-lg border-0 bg-n-alpha-black2 px-3 py-2 text-sm text-n-slate-12 outline outline-1 outline-n-weak transition-colors focus:outline-n-brand";

export default function AccountSettingsPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [account, setAccount] = useState<AccountSettings | null>(null);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let active = true;
    setAccount(null);
    api<AccountSettings>(acct(accountId))
      .then((value) => {
        if (active) setAccount(value);
      })
      .catch((e) => {
        if (active) setError(e.message);
      });
    return () => {
      active = false;
    };
  }, [accountId]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (!account) return;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const updated = await api<AccountSettings>(acct(accountId), {
        method: "PATCH",
        json: {
          name: account.name,
          domain: account.domain,
          support_email: account.support_email,
        },
      });
      setAccount(updated);
      setMessage(t("GENERAL_SETTINGS.UPDATE.SUCCESS"));
    } catch (e) {
      setError(e instanceof Error ? e.message : t("GENERAL_SETTINGS.UPDATE.ERROR"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="flex-1 overflow-auto bg-n-surface-1 px-6 py-6 text-n-slate-12">
      <div className="mx-auto max-w-4xl">
        <header className="border-b border-n-weak pb-4">
          <h1 className="font-inter-display text-xl font-semibold text-n-slate-12">
            {t("GENERAL_SETTINGS.TITLE")}
          </h1>
        </header>

        {error && (
          <p role="alert" className="mt-4 rounded-lg bg-n-ruby-3 px-3 py-2 text-sm text-n-ruby-11">
            {error}
          </p>
        )}
        {message && (
          <p role="status" className="mt-4 rounded-lg bg-n-teal-3 px-3 py-2 text-sm text-n-teal-11">
            {message}
          </p>
        )}

        {/* Section 1: General Settings (Chatwoot SectionLayout 2-col) */}
        <div className="grid gap-6 border-b border-n-weak py-6 md:grid-cols-[18rem_1fr]">
          <div>
            <h2 className="text-base font-medium text-n-slate-12">
              {t("GENERAL_SETTINGS.FORM.GENERAL_SECTION.TITLE")}
            </h2>
            <p className="mt-1 text-xs leading-relaxed text-n-slate-11">
              {t("GENERAL_SETTINGS.FORM.GENERAL_SECTION.NOTE")}
            </p>
          </div>
          {account && (
            <form className="grid max-w-xl gap-4 rounded-xl border border-n-weak bg-n-solid-1 p-5" onSubmit={save}>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>{t("GENERAL_SETTINGS.FORM.NAME.LABEL")}</span>
                <input
                  className={inputCls}
                  required
                  value={account.name}
                  placeholder={t("GENERAL_SETTINGS.FORM.NAME.PLACEHOLDER")}
                  onChange={(e) => setAccount({ ...account, name: e.target.value })}
                />
              </label>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>{t("GENERAL_SETTINGS.FORM.LANGUAGE.LABEL")}</span>
                <input className={inputCls} value={account.locale} readOnly />
              </label>
              {account.features?.inbound_emails && account.features?.custom_reply_domain && (
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>{t("GENERAL_SETTINGS.FORM.DOMAIN.LABEL")}</span>
                  <input
                    className={inputCls}
                    value={account.domain ?? ""}
                    placeholder={t("GENERAL_SETTINGS.FORM.DOMAIN.PLACEHOLDER")}
                    onChange={(e) => setAccount({ ...account, domain: e.target.value })}
                  />
                </label>
              )}
              {account.features?.inbound_emails && account.features?.custom_reply_email && (
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>{t("GENERAL_SETTINGS.FORM.SUPPORT_EMAIL.LABEL")}</span>
                  <input
                    className={inputCls}
                    type="email"
                    value={account.support_email ?? ""}
                    placeholder={t("GENERAL_SETTINGS.FORM.SUPPORT_EMAIL.PLACEHOLDER")}
                    onChange={(e) => setAccount({ ...account, support_email: e.target.value })}
                  />
                </label>
              )}
              <div className="pt-1">
                <Button type="submit" disabled={busy} label={t("GENERAL_SETTINGS.SUBMIT")} />
              </div>
            </form>
          )}
        </div>

        {/* Section 2: Account ID */}
        <div className="grid gap-6 py-6 md:grid-cols-[18rem_1fr]">
          <div>
            <h2 className="text-base font-medium text-n-slate-12">
              {t("GENERAL_SETTINGS.FORM.ACCOUNT_ID.TITLE")}
            </h2>
            <p className="mt-1 text-xs leading-relaxed text-n-slate-11">
              {t("GENERAL_SETTINGS.FORM.ACCOUNT_ID.NOTE")}
            </p>
          </div>
          <div className="flex items-center">
            <span className="inline-flex rounded-lg border border-n-weak bg-n-solid-1 px-3 py-1.5 font-mono text-sm font-medium text-n-slate-12">
              {accountId}
            </span>
          </div>
        </div>
      </div>
    </section>
  );
}
