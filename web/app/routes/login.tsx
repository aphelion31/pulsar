import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { Button, ErrorText, Input } from "~/components/ui";
import { signIn } from "~/lib/api";
import { t } from "~/lib/i18n";

export default function Login() {
  const nav = useNavigate();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    setBusy(true);
    setError("");
    try {
      const me = await signIn(String(f.get("email")), String(f.get("password")));
      nav(`/accounts/${me.account_id}/conversations`, { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Sign in failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-dvh place-items-center px-6">
      <div className="w-full max-w-sm">
        <p className="text-[15px] font-semibold tracking-tight">Pulsar</p>
        <h1 className="mt-8 text-2xl font-medium">Sign in to your inbox</h1>
        <p className="mt-2 text-ink-2">Every customer conversation, in one place</p>
        <form onSubmit={submit} className="mt-8 grid gap-4">
          <Input label="Email" name="email" type="email" autoComplete="email" required autoFocus />
          <Input label="Password" name="password" type="password" autoComplete="current-password" required />
          <Link className="text-sm text-ink-2 underline" to="/auth/reset/password">{t("LOGIN.FORGOT_PASSWORD")}</Link>
          <ErrorText>{error}</ErrorText>
          <Button type="submit" disabled={busy} className="mt-2 h-9 justify-center">
            {busy ? "Signing in" : "Sign in"}
          </Button>
        </form>
      </div>
    </main>
  );
}
