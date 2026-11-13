import { useState } from "react";
import { Link } from "react-router";
import { Button, ErrorText, Input } from "~/components/ui";
import { api } from "~/lib/api";
import { t } from "~/lib/i18n";

export default function ResetPassword() {
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setBusy(true); setError(""); setMessage("");
    try {
      await api("/auth/password", { method: "POST", json: { email: new FormData(e.currentTarget).get("email") } });
      setMessage(t("RESET_PASSWORD.API.SUCCESS_MESSAGE"));
    } catch { setError(t("RESET_PASSWORD.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  return <main className="grid min-h-dvh place-items-center px-6"><div className="w-full max-w-sm">
    <p className="text-[15px] font-semibold tracking-tight">Pulsar</p>
    <h1 className="mt-8 text-2xl font-medium">{t("RESET_PASSWORD.TITLE")}</h1>
    <p className="mt-2 text-ink-2">{t("RESET_PASSWORD.DESCRIPTION")}</p>
    <form onSubmit={submit} className="mt-8 grid gap-4">
      <Input label={t("RESET_PASSWORD.EMAIL.LABEL")} placeholder={t("RESET_PASSWORD.EMAIL.PLACEHOLDER")} name="email" type="email" autoComplete="email" required autoFocus />
      <ErrorText>{error}</ErrorText><p role="status">{message}</p>
      <Button type="submit" disabled={busy} className="mt-2 h-9 justify-center">{t("RESET_PASSWORD.SUBMIT")}</Button>
    </form>
    <p className="mt-4 text-sm text-ink-2">{t("RESET_PASSWORD.GO_BACK_TO_LOGIN")} <Link className="underline" to="/login">{t("LOGIN.SUBMIT")}</Link></p>
  </div></main>;
}
