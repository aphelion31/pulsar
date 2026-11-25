import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { Button } from "~/components/ui";

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
        body: JSON.stringify({ user: { company: company.trim(), name: name.trim(), email: email.trim(), password } }),
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
    return <div className="flex min-h-screen items-center justify-center bg-bg text-sm text-ink-3">Checking installation state...</div>;
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-bg p-6">
      <form onSubmit={submit} className="w-full max-w-md space-y-4 rounded-2xl border border-line bg-surface p-6 shadow-sm">
        <div>
          <h1 className="text-xl font-semibold text-ink">Welcome to Pulsar</h1>
          <p className="mt-1 text-sm text-ink-3">Create the initial organization and SuperAdmin account for this installation.</p>
        </div>
        {error && <p role="alert" className="rounded-lg bg-bad/10 px-3 py-2 text-sm text-bad">{error}</p>}
        <label className="flex flex-col gap-1 text-sm font-medium text-ink">
          Company / Workspace Name
          <input required value={company} onChange={(e) => setCompany(e.target.value)} placeholder="Acme Corp" className="h-10 rounded-lg border border-line bg-bg px-3 text-sm" />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-ink">
          Your Name
          <input required value={name} onChange={(e) => setName(e.target.value)} placeholder="Ada Lovelace" className="h-10 rounded-lg border border-line bg-bg px-3 text-sm" />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-ink">
          Admin Email
          <input required type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="admin@example.com" className="h-10 rounded-lg border border-line bg-bg px-3 text-sm" />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-ink">
          Password (min 6 chars)
          <input required type="password" minLength={6} value={password} onChange={(e) => setPassword(e.target.value)} className="h-10 rounded-lg border border-line bg-bg px-3 text-sm" />
        </label>
        <Button type="submit" disabled={busy} className="w-full">
          {busy ? "Creating installation..." : "Complete Installation"}
        </Button>
      </form>
    </div>
  );
}
