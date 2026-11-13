// Onboarding account details. mirrors chatwoot/app/javascript/dashboard/routes/dashboard/onboarding/Index.vue.
// Backend: PUT /api/v1/accounts/:id/onboarding {onboarding_step: "account_details", name, locale, website, industry,
// company_size, timezone, referral_source, user_role}. Admin only. Success routes to the dashboard.
import { useState, type FormEvent } from "react";
import { useNavigate, useOutletContext } from "react-router";
import { Button, ErrorText, Input, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

const ROLES = [
  { value: "founder", label: "Founder/CEO" },
  { value: "product_manager", label: "Product Manager" },
  { value: "engineering", label: "Engineering" },
  { value: "support_lead", label: "Support Lead" },
  { value: "marketing", label: "Marketing" },
  { value: "sales", label: "Sales" },
  { value: "other", label: "Other" },
];

export default function OnboardingAccountDetails() {
  const { me, accountId } = useOutletContext<ShellCtx>();
  const nav = useNavigate();
  const [name, setName] = useState("");
  const [locale, setLocale] = useState("en");
  const [website, setWebsite] = useState("");
  const [timezone, setTimezone] = useState(Intl.DateTimeFormat().resolvedOptions().timeZone);
  const [role, setRole] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [touched, setTouched] = useState(false);

  const missing = !role || !name.trim();

  async function submit(e: FormEvent) {
    e.preventDefault();
    setTouched(true);
    if (missing || busy) return;
    setBusy(true);
    setError("");
    const url = website.trim() && !/^https?:\/\//i.test(website.trim()) ? `https://${website.trim()}` : website.trim();
    try {
      await api(`${acct(accountId)}/onboarding`, {
        method: "PUT",
        json: {
          onboarding_step: "account_details",
          name,
          locale,
          website: url,
          timezone,
          user_role: role,
        },
      });
      nav(`/accounts/${accountId}/dashboard`);
    } catch {
      setError(t("ONBOARDING_NEXT.ERROR"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col">
      <PageHeader title={t("ONBOARDING_NEXT.YOUR_DETAILS")} />
      <form onSubmit={submit} noValidate className="mx-auto grid w-full max-w-xl gap-5 overflow-y-auto p-8">
        <div>
          <h1 className="text-lg font-medium">{t("ONBOARDING_NEXT.GREETING", { name: me.name })}</h1>
          <p className="mt-1 text-[13px] text-ink-3">{t("ONBOARDING_NEXT.SUBTITLE")}</p>
        </div>

        <h2 className="text-[13px] font-medium text-ink-2">{t("ONBOARDING_NEXT.YOUR_DETAILS")}</h2>
        <Input label={t("ONBOARDING_NEXT.FIELDS.EMAIL")} value={me.email} disabled readOnly />
        <label className="grid gap-1.5">
          <span className="text-[13px] text-ink-2">{t("ONBOARDING_NEXT.FIELDS.YOUR_ROLE")}</span>
          <select value={role} onChange={(e) => setRole(e.target.value)} className="h-9 rounded-lg border border-line-strong bg-surface px-3 text-[14px]">
            <option value="">{t("ONBOARDING_NEXT.PLACEHOLDERS.SELECT_ROLE")}</option>
            {ROLES.map((r) => (
              <option key={r.value} value={r.value}>{r.label}</option>
            ))}
          </select>
          {touched && !role && <ErrorText>{t("ONBOARDING_NEXT.VALIDATION_ERROR")}</ErrorText>}
        </label>

        <h2 className="text-[13px] font-medium text-ink-2">{t("ONBOARDING_NEXT.COMPANY_DETAILS")}</h2>
        <Input label="Account name" value={name} onChange={(e) => setName(e.target.value)} />
        {touched && !name.trim() && <ErrorText>{t("ONBOARDING_NEXT.VALIDATION_ERROR")}</ErrorText>}
        <Input label={t("ONBOARDING_NEXT.FIELDS.WEBSITE")} placeholder={t("ONBOARDING_NEXT.PLACEHOLDERS.ENTER_WEBSITE")} value={website} onChange={(e) => setWebsite(e.target.value)} />
        <Input label={t("ONBOARDING_NEXT.FIELDS.TIMEZONE")} value={timezone} onChange={(e) => setTimezone(e.target.value)} />
        <Input label={t("ONBOARDING_NEXT.FIELDS.LANGUAGE")} value={locale} onChange={(e) => setLocale(e.target.value)} />

        <ErrorText>{error}</ErrorText>
        <div className="flex justify-end">
          <Button type="submit" disabled={busy}>{busy ? t("ONBOARDING_NEXT.SAVING") : t("ONBOARDING_NEXT.CONTINUE")}</Button>
        </div>
      </form>
    </div>
  );
}
