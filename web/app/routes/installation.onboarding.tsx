import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { AuthShell } from "~/components/next/AuthShell";

const inputCls =
  "block h-10 w-full rounded-lg border-0 bg-n-alpha-black2 px-3 py-2 text-sm text-n-slate-12 outline outline-1 outline-n-weak transition-colors placeholder:text-n-slate-10 hover:outline-n-slate-6 focus:outline-n-brand";

export default function InstallationOnboardingRoute() {
  const nav = useNavigate();
  const [company, setCompany] = useState("");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [needed, setNeeded] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    fetch("/installation/onboarding", { headers: { Accept: "application/json" } })
      .then((r) => r.json())
      .then((d: { onboarding_needed?: boolean }) => {
        if (!active) return;
        if (d.onboarding_needed === false) {
          nav("/login", { replace: true });
        } else {
          setNeeded(true);
        }
      })
      .catch(() => {
        if (active) setNeeded(true);
      });
    return () => {
      active = false;
    };
  }, [nav]);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const res = await fetch("/installation/onboarding", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          user: { company: company.trim(), name: name.trim(), email: email.trim(), password },
        }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || data.message || "Onboarding failed");
      const token = res.headers.get("access-token");
      const client = res.headers.get("client");
      const uid = res.headers.get("uid");
      if (token && client && uid) {
        localStorage.setItem("pulsar.session", JSON.stringify({ "access-token": token, client, uid }));
      }
      nav(`/accounts/${data.account?.id ?? 1}/dashboard`, { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Onboarding failed");
    } finally {
      setBusy(false);
    }
  }

  if (needed === null) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-n-background text-sm text-n-slate-10">
        Checking installation state...
      </div>
    );
  }

  return (
    <AuthShell
      title="Welcome to Pulsar"
      subtitle="Create the initial workspace and SuperAdmin account for this installation."
    >
      <form onSubmit={submit} className="space-y-4">
        {error && (
          <p role="alert" className="rounded-lg bg-n-ruby-3 px-3 py-2 text-sm text-n-ruby-11">
            {error}
          </p>
        )}
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Company / Workspace Name</span>
          <input
            required
            autoFocus
            value={company}
            onChange={(e) => setCompany(e.target.value)}
            placeholder="Acme Corp"
            className={inputCls}
          />
        </label>
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Your Name</span>
          <input
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Ada Lovelace"
            className={inputCls}
          />
        </label>
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Admin Email</span>
          <input
            required
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="admin@example.com"
            className={inputCls}
          />
        </label>
        <label className="block space-y-1.5">
          <span className="text-sm font-medium text-n-slate-12">Password (min 6 chars)</span>
          <input
            required
            type="password"
            minLength={6}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Strong password"
            className={inputCls}
          />
        </label>
        <button
          type="submit"
          disabled={busy}
          className="mt-2 flex h-10 w-full items-center justify-center rounded-lg bg-n-brand px-4 text-sm font-medium text-white transition hover:bg-n-blue-10 disabled:opacity-50"
        >
          {busy ? "Creating installation..." : "Complete Installation"}
        </button>
      </form>
    </AuthShell>
  );
}
