import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { AuthShell } from "~/components/next/AuthShell";
import { authResponse } from "~/lib/api";

const inputCls =
  "block h-10 w-full rounded-lg border-0 bg-n-alpha-black2 px-3 py-2 text-sm text-n-slate-12 outline outline-1 outline-n-weak transition-colors placeholder:text-n-slate-10 hover:outline-n-slate-6 focus:outline-n-brand";

export default function SignupRoute() {
  const navigate = useNavigate();
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    const password = String(f.get("password") ?? "");
    const confirm = String(f.get("password_confirmation") ?? "");
    if (password !== confirm) {
      setError("Passwords do not match");
      return;
    }
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const res = await fetch("/api/v1/accounts", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          account_name: String(f.get("account_name") ?? ""),
          user_full_name: String(f.get("user_full_name") ?? ""),
          email: String(f.get("email") ?? ""),
          password,
        }),
      });
      if (res.headers.get("access-token")) {
        const profile = await authResponse(res);
        navigate(`/accounts/${profile.account_id}/conversations`, { replace: true });
        return;
      }
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.message || data.error || "Sign up failed");
      setMessage("Account created. Check your email to confirm your account or sign in.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Sign up failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <AuthShell
      title="Create your account"
      subtitle={
        <>
          Already have an account?{" "}
          <Link className="text-n-brand hover:underline" to="/login">
            Sign in
          </Link>
        </>
      }
    >
      <form onSubmit={submit} className="space-y-4">
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Full name</span>
          <input name="user_full_name" required autoFocus placeholder="Jane Doe" className={inputCls} />
        </label>
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Company / Account name</span>
          <input name="account_name" required placeholder="Acme Inc" className={inputCls} />
        </label>
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Work email</span>
          <input name="email" type="email" autoComplete="email" required placeholder="you@company.com" className={inputCls} />
        </label>
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Password</span>
          <input name="password" type="password" minLength={6} required placeholder="At least 6 characters" className={inputCls} />
        </label>
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Confirm password</span>
          <input name="password_confirmation" type="password" minLength={6} required placeholder="Repeat password" className={inputCls} />
        </label>
        {error && <p role="alert" className="text-sm text-n-ruby-11">{error}</p>}
        {message && <p role="status" className="text-sm text-n-teal-11">{message}</p>}
        <button
          type="submit"
          disabled={busy}
          className="mt-2 flex h-10 w-full items-center justify-center rounded-lg bg-n-brand px-4 text-sm font-medium text-white transition hover:bg-n-blue-10 disabled:opacity-50"
        >
          {busy ? "Creating account..." : "Create account"}
        </button>
      </form>
    </AuthShell>
  );
}
