import { LockKeyhole } from "lucide-react";
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { AuthShell } from "~/components/next/AuthShell";

const inputCls =
  "block h-10 w-full rounded-lg border-0 bg-n-alpha-black2 px-3 py-2 text-sm text-n-slate-12 outline outline-1 outline-n-weak transition-colors placeholder:text-n-slate-10 hover:outline-n-slate-6 focus:outline-n-brand";

export default function SsoLoginRoute() {
  const [params] = useSearchParams();
  const [email, setEmail] = useState("");
  const [error, setError] = useState(
    params.get("error") === "saml-authentication-failed"
      ? "SAML authentication failed for that email."
      : ""
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
    <AuthShell
      title="Sign in with SSO"
      subtitle="Enter your work email to continue with SAML single sign-on."
    >
      <form onSubmit={submit} className="space-y-5">
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Work email</span>
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="name@company.com"
            required
            autoFocus
            className={inputCls}
          />
        </label>
        {error && <p role="alert" className="text-sm text-n-ruby-11">{error}</p>}
        <button
          type="submit"
          disabled={busy}
          className="flex h-10 w-full items-center justify-center gap-2 rounded-lg bg-n-brand px-4 text-sm font-medium text-white transition hover:bg-n-blue-10 disabled:opacity-50"
        >
          <LockKeyhole className="size-4" />
          {busy ? "Redirecting..." : "Continue with SSO"}
        </button>
      </form>
      <p className="mt-5 text-center text-sm text-n-slate-11">
        <Link className="text-n-brand hover:underline" to="/login">
          Sign in with password instead
        </Link>
      </p>
    </AuthShell>
  );
}
