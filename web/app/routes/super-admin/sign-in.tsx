import { useState } from "react";
import { useNavigate } from "react-router";
import { Button } from "~/components/ui";

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
    <div className="flex min-h-screen items-center justify-center bg-bg p-6">
      <form onSubmit={submit} className="w-full max-w-md space-y-4 rounded-2xl border border-line bg-surface p-6">
        <div>
          <h1 className="text-xl font-semibold text-ink">Super Admin Sign In</h1>
          <p className="mt-1 text-sm text-ink-3">Instance-wide administration for Pulsar.</p>
        </div>
        {error && <p role="alert" className="rounded-lg bg-bad/10 px-3 py-2 text-sm text-bad">{error}</p>}
        <label className="flex flex-col gap-1 text-sm font-medium text-ink">
          Email
          <input required type="email" value={email} onChange={(e) => setEmail(e.target.value)} className="h-10 rounded-lg border border-line bg-bg px-3 text-sm" />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-ink">
          Password
          <input required type="password" value={password} onChange={(e) => setPassword(e.target.value)} className="h-10 rounded-lg border border-line bg-bg px-3 text-sm" />
        </label>
        <Button type="submit" disabled={busy} className="w-full">
          {busy ? "Signing in..." : "Sign in"}
        </Button>
      </form>
    </div>
  );
}
