import { ArrowLeft } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../../shell";

type AppInfo = {
  id: string;
  name: string;
  description?: string;
  hooks?: Array<{ id: number; reference_id?: string | null; settings?: { workspace_name?: string } }>;
};

export default function NotionIntegrationPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = acct(accountId);
  const [app, setApp] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    const info = await api<AppInfo>(`${base}/integrations/apps/notion`);
    setApp(info);
  }, [base]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    load()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load Notion");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [load]);

  const hook = app?.hooks?.[0];

  async function handleConnect() {
    setBusy(true);
    setError("");
    try {
      const r = await api<{ url: string }>(`${base}/notion/authorization`, { method: "POST", json: {} });
      window.location.href = r.url;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start Notion OAuth");
      setBusy(false);
    }
  }

  async function handleDisconnect() {
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/notion`, { method: "DELETE" });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to disconnect Notion");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 sm:p-8 text-n-slate-12">
      <Link to={`/accounts/${accountId}/settings/integrations`} className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12 mb-4">
        <ArrowLeft className="size-4" /> Integrations
      </Link>
      <h1 className="text-xl font-medium mb-2">{app?.name || "Notion"}</h1>
      {app?.description && <p className="text-sm text-n-slate-11 mb-6">{app.description}</p>}
      {error && <p role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">{error}</p>}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading...</p>
      ) : hook ? (
        <div className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <div className="text-sm font-medium text-n-teal-11">
            Connected to workspace: {hook.settings?.workspace_name || hook.reference_id || "Notion"}
          </div>
          <Button solid ruby isLoading={busy} onClick={handleDisconnect} label="Disconnect Notion" />
        </div>
      ) : (
        <div className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <Button solid blue isLoading={busy} onClick={handleConnect} label="Connect Notion" />
        </div>
      )}
    </div>
  );
}
