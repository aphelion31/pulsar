import { useState } from "react";
import { Link } from "react-router";
import { AuthShell } from "~/components/next/AuthShell";
import { api } from "~/lib/api";
import { t } from "~/lib/i18n";

const inputCls =
  "block h-10 w-full rounded-lg border-0 bg-n-alpha-black2 px-3 py-2 text-sm text-n-slate-12 outline outline-1 outline-n-weak transition-colors placeholder:text-n-slate-10 hover:outline-n-slate-6 focus:outline-n-brand";

export default function ResetPassword() {
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await api("/auth/password", {
        method: "POST",
        json: { email: new FormData(e.currentTarget).get("email") },
      });
      setMessage(t("RESET_PASSWORD.API.SUCCESS_MESSAGE"));
    } catch {
      setError(t("RESET_PASSWORD.API.ERROR_MESSAGE"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <AuthShell
      title={t("RESET_PASSWORD.TITLE")}
      subtitle={t("RESET_PASSWORD.DESCRIPTION")}
    >
      <form onSubmit={submit} className="space-y-5">
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">
            {t("RESET_PASSWORD.EMAIL.LABEL")}
          </span>
          <input
            name="email"
            type="email"
            autoComplete="email"
            required
            autoFocus
            placeholder={t("RESET_PASSWORD.EMAIL.PLACEHOLDER")}
            className={inputCls}
          />
        </label>
        {error && <p role="alert" className="text-sm text-n-ruby-11">{error}</p>}
        {message && <p role="status" className="text-sm text-n-teal-11">{message}</p>}
        <button
          type="submit"
          disabled={busy}
          className="flex h-10 w-full items-center justify-center rounded-lg bg-n-brand px-4 text-sm font-medium text-white transition hover:bg-n-blue-10 disabled:opacity-50"
        >
          {t("RESET_PASSWORD.SUBMIT")}
        </button>
      </form>
      <p className="mt-5 text-center text-sm text-n-slate-11">
        {t("RESET_PASSWORD.GO_BACK_TO_LOGIN")}{" "}
        <Link className="text-n-brand hover:underline" to="/login">
          {t("LOGIN.SUBMIT")}
        </Link>
      </p>
    </AuthShell>
  );
}
