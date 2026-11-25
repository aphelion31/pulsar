import { Activity, Eye, Pause, Play, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import { Button, Empty } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "./shell";

type Monitor = {
  id: number;
  name: string;
  condition: string;
  icon: string;
  icon_color: string;
  paused_at: number | null;
  recent_count: number;
  processing: { state: string; errors: number };
};

type MonitorsIndex = {
  payload: Monitor[];
  usage: { used: number; limit: number; remaining: number };
};

export default function MonitorsRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [monitors, setMonitors] = useState<Monitor[]>([]);
  const [usage, setUsage] = useState<{ used: number; limit: number; remaining: number } | null>(null);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [condition, setCondition] = useState("");
  const [iconColor, setIconColor] = useState("#3B82F6");
  const [previewCount, setPreviewCount] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function load() {
    setLoading(true);
    setError("");
    try {
      const res = await api<MonitorsIndex>(`${acct(accountId)}/monitors`);
      setMonitors(res.payload ?? []);
      setUsage(res.usage ?? null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load monitors");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId]);

  async function runPreview() {
    if (!condition.trim()) return;
    setBusy(true);
    setError("");
    try {
      const res = await api<{ matched_count: number }>(`${acct(accountId)}/monitors/preview`, {
        method: "POST",
        json: { condition: condition.trim() },
      });
      setPreviewCount(res.matched_count ?? 0);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Preview failed");
    } finally {
      setBusy(false);
    }
  }

  async function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim() || !condition.trim()) return;
    setBusy(true);
    setError("");
    try {
      await api(`${acct(accountId)}/monitors`, {
        method: "POST",
        json: { name: name.trim(), condition: condition.trim(), icon_color: iconColor },
      });
      setName("");
      setCondition("");
      setPreviewCount(null);
      setShowCreate(false);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not create monitor");
    } finally {
      setBusy(false);
    }
  }

  async function togglePause(m: Monitor) {
    setError("");
    try {
      if (m.paused_at) {
        await api(`${acct(accountId)}/monitors/${m.id}/resume`, {
          method: "POST",
          json: { mode: "continue", collection_version: 0 },
        });
      } else {
        await api(`${acct(accountId)}/monitors/${m.id}`, {
          method: "PATCH",
          json: { paused: true },
        });
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to update monitor");
    }
  }

  async function removeMonitor(id: number) {
    setError("");
    try {
      await api(`${acct(accountId)}/monitors/${id}`, { method: "DELETE" });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete monitor");
    }
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg p-6">
      <div className="mx-auto w-full max-w-6xl space-y-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold text-ink">Conversation Monitors</h1>
            <p className="text-sm text-ink-3">Track recurring topics, churn signals, and quality rules across conversations.</p>
          </div>
          <div className="flex items-center gap-3">
            {usage && (
              <span className="rounded-lg border border-line bg-surface px-3 py-1.5 text-xs text-ink-2">
                Daily evaluations: <strong className="font-mono text-ink">{usage.used}</strong> / {usage.limit}
              </span>
            )}
            <Button onClick={() => setShowCreate((v) => !v)}>
              <Plus size={15} /> New Monitor
            </Button>
          </div>
        </div>

        {showCreate && (
          <form onSubmit={handleCreate} className="space-y-4 rounded-xl border border-line bg-surface p-5">
            <div className="grid gap-4 md:grid-cols-3">
              <label className="flex flex-col gap-1 text-sm font-medium text-ink md:col-span-2">
                Monitor Name
                <input
                  required
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Billing Refund Requests"
                  className="h-10 rounded-lg border border-line bg-bg px-3 text-sm"
                />
              </label>
              <label className="flex flex-col gap-1 text-sm font-medium text-ink">
                Badge Color
                <input
                  type="color"
                  value={iconColor}
                  onChange={(e) => setIconColor(e.target.value)}
                  className="h-10 w-full rounded-lg border border-line bg-bg p-1"
                />
              </label>
            </div>
            <label className="flex flex-col gap-1 text-sm font-medium text-ink">
              Condition
              <textarea
                required
                rows={3}
                value={condition}
                onChange={(e) => setCondition(e.target.value)}
                placeholder="Customer asks for a refund, chargeback, or billing cancellation"
                className="rounded-lg border border-line bg-bg p-3 text-sm"
              />
            </label>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                <Button type="button" variant="quiet" onClick={() => void runPreview()} disabled={busy || !condition.trim()}>
                  <Eye size={15} /> Preview Matches
                </Button>
                {previewCount !== null && (
                  <span className="text-xs text-ink-2">
                    Matched <strong className="font-mono text-ink">{previewCount}</strong> recent conversations
                  </span>
                )}
              </div>
              <div className="flex gap-2">
                <Button type="button" variant="quiet" onClick={() => setShowCreate(false)}>
                  Cancel
                </Button>
                <Button type="submit" disabled={busy}>
                  Create Monitor
                </Button>
              </div>
            </div>
          </form>
        )}

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        {loading ? (
          <div className="rounded-xl border border-line bg-surface p-8 text-center text-sm text-ink-3">Loading monitors...</div>
        ) : monitors.length === 0 ? (
          <Empty title="No conversation monitors">
            <p className="text-sm text-ink-3">Create a monitor to automatically flag conversations matching a natural-language condition.</p>
          </Empty>
        ) : (
          <div className="grid gap-4 md:grid-cols-2">
            {monitors.map((m) => (
              <div key={m.id} className="flex flex-col justify-between rounded-xl border border-line bg-surface p-5">
                <div className="space-y-2">
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex items-center gap-2.5">
                      <span
                        className="flex size-8 items-center justify-center rounded-lg text-white"
                        style={{ backgroundColor: m.icon_color || "#3B82F6" }}
                      >
                        <Activity size={16} />
                      </span>
                      <div>
                        <Link
                          to={`/accounts/${accountId}/reports/monitors/${m.id}`}
                          className="font-medium text-ink hover:text-accent hover:underline"
                        >
                          {m.name}
                        </Link>
                        <div className="text-xs capitalize text-ink-3">{m.processing.state}</div>
                      </div>
                    </div>
                    <span className="rounded-md bg-raised px-2 py-0.5 font-mono text-xs text-ink-2">
                      {m.recent_count} matches (7d)
                    </span>
                  </div>
                  <p className="text-sm text-ink-2">{m.condition}</p>
                </div>
                <div className="mt-4 flex items-center justify-between border-t border-line pt-3 text-xs">
                  <Link
                    to={`/accounts/${accountId}/reports/monitors/${m.id}`}
                    className="font-medium text-accent hover:underline"
                  >
                    View Report
                  </Link>
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => void togglePause(m)}
                      className="inline-flex items-center gap-1 text-ink-2 hover:text-ink"
                    >
                      {m.paused_at ? <Play size={13} /> : <Pause size={13} />}
                      {m.paused_at ? "Resume" : "Pause"}
                    </button>
                    <button
                      onClick={() => void removeMonitor(m.id)}
                      className="inline-flex items-center gap-1 text-bad hover:underline"
                    >
                      <Trash2 size={13} /> Delete
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
