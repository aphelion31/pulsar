import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Button, ErrorText, Input } from "~/components/ui";

export default function SsoLoginRoute() {
  const [params] = useSearchParams();
  const [email, setEmail] = useState("");
  const [error, setError] = useState(
    params.get("error") === "saml-authentication-failed" ? "SAML authentication failed for that email." : "",
  );
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const res = await fetch("/api/v1/auth/saml_login", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          email: email.trim(),
          redirect_url: params.get("redirect_url") ?? undefined,
          sso_account_id: params.get("sso_account_id") ?? undefined,
        }),
      });
      if (res.redirected && res.url) {
        window.location.assign(res.url);
        return;
      }
      const data = await res.json().catch(() => ({}));
      throw new Error(data.error || "SSO login failed");
    } catch (err) {
      setError(err instanceof Error ? err.message : "SSO login failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-dvh place-items-center px-6">
      <div className="w-full max-w-sm">
        <p className="text-[15px] font-semibold tracking-tight">Pulsar</p>
        <h1 className="mt-8 text-2xl font-medium">Sign in with SSO</h1>
        <p className="mt-2 text-ink-2">Enter your work email to continue with SAML single sign-on.</p>
        <form onSubmit={submit} className="mt-8 grid gap-4">
          <Input label="Work email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoFocus />
          <ErrorText>{error}</ErrorText>
          <Button type="submit" disabled={busy} className="h-9 justify-center">
            {busy ? "Redirecting..." : "Continue with SSO"}
          </Button>
        </form>
        <p className="mt-4 text-sm text-ink-2">
          <Link className="underline" to="/login">
            Sign in with password instead
          </Link>
        </p>
      </div>
    </main>
  );
}
