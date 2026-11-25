import { useState } from "react";
import { useNavigate } from "react-router";
import { AuthShell } from "~/components/next/AuthShell";

const inputCls =
  "block h-10 w-full rounded-lg border-0 bg-n-alpha-black2 px-3 py-2 text-sm text-n-slate-12 outline outline-1 outline-n-weak transition-colors placeholder:text-n-slate-10 hover:outline-n-slate-6 focus:outline-n-brand";

export default function SuperAdminSignInRoute() {
  const nav = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const res = await fetch("/super_admin/sign_in", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ email: email.trim(), password }),
      });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.error || data.message || "Sign-in failed");
      const token = res.headers.get("access-token");
      const client = res.headers.get("client");
      const uid = res.headers.get("uid");
      if (token && client && uid) {
        localStorage.setItem("pulsar.session", JSON.stringify({ "access-token": token, client, uid }));
      }
      nav("/super_admin", { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "SuperAdmin sign-in failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <AuthShell
      title="Super Admin Console"
      subtitle="Instance-wide administration for this installation."
    >
      <form onSubmit={submit} className="space-y-5">
        {error && (
          <p role="alert" className="rounded-lg bg-n-ruby-3 px-3 py-2 text-sm text-n-ruby-11">
            {error}
          </p>
        )}
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Email</span>
          <input
            required
            autoFocus
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="super@company.com"
            className={inputCls}
          />
        </label>
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Password</span>
          <input
            required
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Password"
            className={inputCls}
          />
        </label>
        <button
          type="submit"
          disabled={busy}
          className="flex h-10 w-full items-center justify-center rounded-lg bg-n-brand px-4 text-sm font-medium text-white transition hover:bg-n-blue-10 disabled:opacity-50"
        >
          {busy ? "Signing in..." : "Sign in"}
        </button>
      </form>
    </AuthShell>
  );
}
