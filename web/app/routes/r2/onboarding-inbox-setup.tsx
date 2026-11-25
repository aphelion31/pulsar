// Onboarding inbox setup. mirrors chatwoot/app/javascript/dashboard/routes/dashboard/onboarding/InboxSetup.vue.
// Pulsar self-host has no cloud channel auto-creation, so this step lists the next actions and finishes
// onboarding with PUT /api/v1/accounts/:id/onboarding {onboarding_step: "inbox_setup"} (no-op when cursor differs).
import { useState } from "react";
import { useNavigate, useOutletContext } from "react-router";
import { Button, ErrorText, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

export default function OnboardingInboxSetup() {
  const { accountId } = useOutletContext<ShellCtx>();
  const nav = useNavigate();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function finish() {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await api(`${acct(accountId)}/onboarding`, { method: "PUT", json: { onboarding_step: "inbox_setup" } });
      nav(`/accounts/${accountId}/dashboard`);
    } catch {
      setError(t("ONBOARDING_INBOX_SETUP.ERROR"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col">
      <PageHeader title={t("ONBOARDING_INBOX_SETUP.GREETING")} />
      <div className="mx-auto grid w-full max-w-xl gap-6 p-8">
        <p className="text-[13px] text-ink-3">{t("ONBOARDING_INBOX_SETUP.SUBTITLE")}</p>
        <h2 className="text-[15px] font-medium">{t("ONBOARDING_INBOX_SETUP.CREATED_FOR_YOU.TITLE")}</h2>
        <ul className="grid gap-3">
          <li className="rounded-lg border border-line p-4">
            <p className="text-[14px] font-medium">{t("ONBOARDING_INBOX_SETUP.CREATED_FOR_YOU.LIVE_CHAT")}</p>
            <p className="text-[13px] text-ink-3">{t("ONBOARDING_INBOX_SETUP.CREATED_FOR_YOU.LIVE_CHAT_DESCRIPTION")}</p>
          </li>
          <li className="rounded-lg border border-line p-4">
            <p className="text-[14px] font-medium">{t("ONBOARDING_INBOX_SETUP.CREATED_FOR_YOU.HELP_CENTER")}</p>
            <p className="text-[13px] text-ink-3">{t("ONBOARDING_INBOX_SETUP.CREATED_FOR_YOU.HELP_CENTER_DESCRIPTION")}</p>
          </li>
        </ul>
        <ErrorText>{error}</ErrorText>
        <div className="flex justify-end gap-2">
          <Button variant="quiet" onClick={finish} disabled={busy}>{t("ONBOARDING_INBOX_SETUP.SKIP")}</Button>
          <Button onClick={finish} disabled={busy}>{t("ONBOARDING_INBOX_SETUP.CONTINUE")}</Button>
        </div>
      </div>
    </div>
  );
}
