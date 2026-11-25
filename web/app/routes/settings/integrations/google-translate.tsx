import { ArrowLeft } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../../shell";

type Hook = { id: number; settings?: { project_id?: string; api_key?: string } };
type AppInfo = { id: string; name: string; description?: string; hooks?: Hook[] };

export default function GoogleTranslateIntegrationPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = acct(accountId);
  const [app, setApp] = useState<AppInfo | null>(null);
  const [projectId, setProjectId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [credentials, setCredentials] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    const info = await api<AppInfo>(`${base}/integrations/apps/google_translate`);
    setApp(info);
    const h = info.hooks?.[0];
    if (h) {
      setProjectId(h.settings?.project_id || "");
      setApiKey(h.settings?.api_key || "");
    }
  }, [base]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    load()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load Google Translate");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [load]);

  const hook = app?.hooks?.[0];

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const settings: Record<string, string> = { project_id: projectId.trim(), api_key: apiKey.trim() };
      if (credentials.trim()) settings.credentials = credentials.trim();
      if (hook) {
        await api(`${base}/integrations/hooks/${hook.id}`, { method: "PATCH", json: { hook: { settings } } });
      } else {
        await api(`${base}/integrations/hooks`, {
          method: "POST",
          json: { hook: { app_id: "google_translate", settings } },
        });
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save configuration");
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete() {
    if (!hook) return;
    setBusy(true);
    try {
      await api(`${base}/integrations/hooks/${hook.id}`, { method: "DELETE" });
      setProjectId("");
      setApiKey("");
      setCredentials("");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete configuration");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 sm:p-8 text-n-slate-12">
      <Link to={`/accounts/${accountId}/settings/integrations`} className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12 mb-4">
        <ArrowLeft className="size-4" /> Integrations
      </Link>
      <h1 className="text-xl font-medium mb-2">{app?.name || "Google Translate"}</h1>
      {app?.description && <p className="text-sm text-n-slate-11 mb-6">{app.description}</p>}
      {error && <p role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">{error}</p>}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading...</p>
      ) : (
        <form onSubmit={handleSave} className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <input required value={projectId} onChange={(e) => setProjectId(e.target.value)} placeholder="GCP Project ID" className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm" />
          <input value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="Translation API Key" className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm font-mono" />
          <textarea rows={4} value={credentials} onChange={(e) => setCredentials(e.target.value)} placeholder="Service Account JSON (optional)" className="w-full rounded-lg border border-n-weak bg-n-background p-3 font-mono text-xs" />
          <div className="flex gap-2">
            <Button solid blue type="submit" isLoading={busy} label={hook ? "Update" : "Enable Google Translate"} />
            {hook && <Button solid ruby type="button" isLoading={busy} onClick={handleDelete} label="Disconnect" />}
          </div>
        </form>
      )}
    </div>
  );
}
