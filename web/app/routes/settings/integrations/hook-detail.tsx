import { ArrowLeft, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import Button from "~/components/next/Button";
import { acct, api, type Inbox } from "~/lib/api";
import type { ShellCtx } from "../../shell";

type Hook = { id: number; inbox_id?: number | null; settings?: Record<string, unknown> };
type AppInfo = {
  id: string;
  name: string;
  description?: string;
  hook_type?: string;
  settings_json_schema?: { properties?: Record<string, { type?: string; title?: string }> };
  hooks?: Hook[];
};

export default function HookDetailIntegrationPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { integration_id = "" } = useParams();
  const base = acct(accountId);
  const [app, setApp] = useState<AppInfo | null>(null);
  const [inboxes, setInboxes] = useState<Inbox[]>([]);
  const [inboxId, setInboxId] = useState("");
  const [fields, setFields] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    const [info, inb] = await Promise.all([
      api<AppInfo>(`${base}/integrations/apps/${integration_id}`),
      api<{ payload: Inbox[] }>(`${base}/inboxes`).catch(() => ({ payload: [] })),
    ]);
    setApp(info);
    setInboxes(inb.payload || []);
  }, [base, integration_id]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    load()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load integration");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [load]);

  const schemaProps = app?.settings_json_schema?.properties || {};
  const schemaKeys = Object.keys(schemaProps);
  const hookFields = schemaKeys.length > 0 ? schemaKeys : ["api_key", "webhook_url"];

  async function handleCreateHook(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/hooks`, {
        method: "POST",
        json: {
          hook: {
            app_id: integration_id,
            inbox_id: inboxId ? Number(inboxId) : undefined,
            settings: fields,
          },
        },
      });
      setFields({});
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to add hook");
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteHook(id: number) {
    setBusy(true);
    try {
      await api(`${base}/integrations/hooks/${id}`, { method: "DELETE" });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete hook");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 sm:p-8 text-n-slate-12 space-y-6">
      <div>
        <Link to={`/accounts/${accountId}/settings/integrations`} className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12 mb-4">
          <ArrowLeft className="size-4" /> Integrations
        </Link>
        <h1 className="text-xl font-medium capitalize">{app?.name || integration_id.replace(/_/g, " ")}</h1>
        {app?.description && <p className="mt-1 text-sm text-n-slate-11">{app.description}</p>}
      </div>

      {error && <p role="alert" className="rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">{error}</p>}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading...</p>
      ) : (
        <div className="max-w-2xl space-y-6">
          <form onSubmit={handleCreateHook} className="space-y-3 rounded-xl border border-n-weak bg-n-solid-1 p-6">
            <h2 className="text-base font-medium">Configure Hook</h2>
            {inboxes.length > 0 && (
              <select value={inboxId} onChange={(e) => setInboxId(e.target.value)} className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm">
                <option value="">Account-wide (no specific inbox)</option>
                {inboxes.map((i) => (
                  <option key={i.id} value={i.id}>{i.name}</option>
                ))}
              </select>
            )}
            {hookFields.map((key) => (
              <input
                key={key}
                value={fields[key] || ""}
                onChange={(e) => setFields((prev) => ({ ...prev, [key]: e.target.value }))}
                placeholder={schemaProps[key]?.title || key}
                className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm"
              />
            ))}
            <Button solid blue type="submit" isLoading={busy} label="Add Hook" />
          </form>

          {(app?.hooks || []).length > 0 && (
            <div className="space-y-2">
              <h2 className="text-sm font-medium text-n-slate-11">Configured Hooks</h2>
              {(app?.hooks || []).map((h) => (
                <div key={h.id} className="flex items-center justify-between rounded-xl border border-n-weak bg-n-solid-1 p-4 text-sm">
                  <div className="font-mono text-xs text-n-slate-11">
                    Hook #{h.id} {h.inbox_id ? `(Inbox #${h.inbox_id})` : ""} — {JSON.stringify(h.settings || {})}
                  </div>
                  <button type="button" onClick={() => handleDeleteHook(h.id)} className="text-n-ruby-11" aria-label={`Delete hook ${h.id}`}>
                    <Trash2 className="size-4" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
