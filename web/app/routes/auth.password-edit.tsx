import { useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { Button, ErrorText, Input } from "~/components/ui";
import { authResponse } from "~/lib/api";
import { t } from "~/lib/i18n";

export default function SetNewPassword() {
  const [params] = useSearchParams();
  const nav = useNavigate();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);
    const password = String(form.get("password"));
    const confirmation = String(form.get("password_confirmation"));
    if (password.length < 6) { setError(t("SET_NEW_PASSWORD.PASSWORD.ERROR")); return; }
    if (password !== confirmation) { setError(t("SET_NEW_PASSWORD.CONFIRM_PASSWORD.ERROR")); return; }
    setBusy(true); setError("");
    try {
      const response = await fetch("/auth/password", { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ reset_password_token: params.get("reset_password_token"), password, password_confirmation: confirmation }) });
      const me = await authResponse(response);
      nav(`/accounts/${me.account_id}/conversations`, { replace: true });
    } catch { setError(t("SET_NEW_PASSWORD.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  return <main className="grid min-h-dvh place-items-center px-6"><div className="w-full max-w-sm">
    <p className="text-[15px] font-semibold tracking-tight">Pulsar</p>
    <h1 className="mt-8 text-2xl font-medium">{t("SET_NEW_PASSWORD.TITLE")}</h1>
    <form onSubmit={submit} className="mt-8 grid gap-4">
      <Input label={t("SET_NEW_PASSWORD.PASSWORD.LABEL")} placeholder={t("SET_NEW_PASSWORD.PASSWORD.PLACEHOLDER")} name="password" type="password" autoComplete="new-password" required autoFocus />
      <Input label={t("SET_NEW_PASSWORD.CONFIRM_PASSWORD.LABEL")} placeholder={t("SET_NEW_PASSWORD.CONFIRM_PASSWORD.PLACEHOLDER")} name="password_confirmation" type="password" autoComplete="new-password" required />
      <ErrorText>{error}</ErrorText>
      <Button type="submit" disabled={busy} className="mt-2 h-9 justify-center">{t("SET_NEW_PASSWORD.SUBMIT")}</Button>
    </form>
  </div></main>;
}
