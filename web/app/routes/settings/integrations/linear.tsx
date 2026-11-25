import { ArrowLeft } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../../shell";

type Team = { id: string; name: string };
type AppInfo = { id: string; name: string; description?: string; hooks?: Array<{ id: number }> };

export default function LinearIntegrationPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = acct(accountId);
  const [app, setApp] = useState<AppInfo | null>(null);
  const [teams, setTeams] = useState<Team[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    const info = await api<AppInfo>(`${base}/integrations/apps/linear`);
    setApp(info);
    if (info.hooks && info.hooks.length > 0) {
      const t = await api<Team[]>(`${base}/integrations/linear/teams`).catch(() => []);
      setTeams(t);
    } else {
      setTeams([]);
    }
  }, [base]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    load()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load Linear");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [load]);

  async function handleConnect() {
    setBusy(true);
    setError("");
    try {
      const r = await api<{ url: string }>(`${base}/integrations/linear/auth`, { method: "POST", json: {} });
      window.location.href = r.url;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start Linear OAuth");
      setBusy(false);
    }
  }

  async function handleDisconnect() {
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/linear`, { method: "DELETE" });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to disconnect Linear");
    } finally {
      setBusy(false);
    }
  }

  const connected = Boolean(app?.hooks && app.hooks.length > 0);

  return (
    <div className="flex-1 overflow-y-auto p-6 sm:p-8 text-n-slate-12">
      <Link to={`/accounts/${accountId}/settings/integrations`} className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12 mb-4">
        <ArrowLeft className="size-4" /> Integrations
      </Link>
      <h1 className="text-xl font-medium mb-2">{app?.name || "Linear"}</h1>
      {app?.description && <p className="text-sm text-n-slate-11 mb-6">{app.description}</p>}
      {error && <p role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">{error}</p>}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading...</p>
      ) : connected ? (
        <div className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <div className="text-sm font-medium text-n-teal-11">Linear connected</div>
          {teams.length > 0 && (
            <div>
              <div className="text-xs text-n-slate-11 mb-2">Available Teams</div>
              <div className="flex flex-wrap gap-2">
                {teams.map((t) => (
                  <span key={t.id} className="rounded-lg border border-n-weak bg-n-background px-2.5 py-1 text-xs">{t.name}</span>
                ))}
              </div>
            </div>
          )}
          <Button solid ruby isLoading={busy} onClick={handleDisconnect} label="Disconnect Linear" />
        </div>
      ) : (
        <div className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <p className="text-sm text-n-slate-11">Connect your Linear workspace to create and link issues directly from conversations.</p>
          <Button solid blue isLoading={busy} onClick={handleConnect} label="Connect Linear" />
        </div>
      )}
    </div>
  );
}
