import { ArrowLeft, Pause, Play, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import { Button } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "./shell";

type MonitorDetail = {
  id: number;
  name: string;
  condition: string;
  icon_color: string;
  slack_channel_id: string | null;
  paused_at: number | null;
  recent_count: number;
  processing: { state: string; errors: number };
};

type Point = { timestamp: number; date: string; value: number };
type MatchedConv = {
  id: number;
  conversation_id: number;
  status: number;
  score: number | null;
  matched_at: number | null;
  contact: { name: string };
  inbox: { name: string };
};

export default function MonitorDetailRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { monitorId } = useParams();
  const [monitor, setMonitor] = useState<MonitorDetail | null>(null);
  const [series, setSeries] = useState<Point[]>([]);
  const [convs, setConvs] = useState<MatchedConv[]>([]);
  const [name, setName] = useState("");
  const [condition, setCondition] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  async function load() {
    if (!monitorId) return;
    setError("");
    try {
      const [m, ts, c] = await Promise.all([
        api<MonitorDetail>(`${acct(accountId)}/monitors/${monitorId}`),
        api<Point[]>(`${acct(accountId)}/monitors/${monitorId}/timeseries`),
        api<{ payload: MatchedConv[] }>(`${acct(accountId)}/monitors/${monitorId}/conversations`),
      ]);
      setMonitor(m);
      setName(m.name);
      setCondition(m.condition);
      setSeries(ts ?? []);
      setConvs(c.payload ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load monitor");
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId, monitorId]);

  async function saveChanges(e: React.FormEvent) {
    e.preventDefault();
    if (!monitorId) return;
    setSaving(true);
    setError("");
    try {
      const updated = await api<MonitorDetail>(`${acct(accountId)}/monitors/${monitorId}`, {
        method: "PATCH",
        json: { name: name.trim(), condition: condition.trim() },
      });
      setMonitor(updated);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not save monitor");
    } finally {
      setSaving(false);
    }
  }

  async function toggleState() {
    if (!monitor || !monitorId) return;
    setError("");
    try {
      if (monitor.paused_at) {
        await api(`${acct(accountId)}/monitors/${monitorId}/resume`, {
          method: "POST",
          json: { mode: "continue", collection_version: 0 },
        });
      } else {
        await api(`${acct(accountId)}/monitors/${monitorId}`, {
          method: "PATCH",
          json: { paused: true },
        });
      }
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to toggle state");
    }
  }

  if (!monitor) {
    return (
      <div className="p-6 text-sm text-ink-3">{error || "Loading monitor..."}</div>
    );
  }

  const maxVal = Math.max(1, ...series.map((p) => p.value));

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg p-6">
      <div className="mx-auto w-full max-w-6xl space-y-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <Link to={`/accounts/${accountId}/reports/monitors`} className="rounded-lg border border-line bg-surface p-2 text-ink-2 hover:text-ink">
              <ArrowLeft size={16} />
            </Link>
            <div>
              <h1 className="text-xl font-semibold text-ink">{monitor.name}</h1>
              <p className="text-xs capitalize text-ink-3">State: {monitor.processing.state} · {monitor.recent_count} matches (7d)</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="quiet" onClick={() => void toggleState()}>
              {monitor.paused_at ? <Play size={14} /> : <Pause size={14} />}
              {monitor.paused_at ? "Resume" : "Pause"}
            </Button>
            <Button variant="quiet" onClick={() => void load()}>
              <RefreshCw size={14} /> Refresh
            </Button>
          </div>
        </div>

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        <form onSubmit={saveChanges} className="space-y-3 rounded-xl border border-line bg-surface p-5">
          <div className="grid gap-3 md:grid-cols-2">
            <label className="flex flex-col gap-1 text-xs font-medium text-ink-2">
              Name
              <input value={name} onChange={(e) => setName(e.target.value)} className="h-9 rounded-lg border border-line bg-bg px-3 text-sm text-ink" />
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink-2">
              Condition
              <input value={condition} onChange={(e) => setCondition(e.target.value)} className="h-9 rounded-lg border border-line bg-bg px-3 text-sm text-ink" />
            </label>
          </div>
          <div className="flex justify-end">
            <Button type="submit" disabled={saving}>{saving ? "Saving..." : "Save Changes"}</Button>
          </div>
        </form>

        <div className="rounded-xl border border-line bg-surface p-5">
          <h2 className="mb-3 text-sm font-medium text-ink">Matches Over Time</h2>
          {series.length === 0 ? (
            <p className="text-sm text-ink-3">No daily matches recorded yet.</p>
          ) : (
            <div className="flex h-36 items-end gap-2">
              {series.map((p) => (
                <div key={p.date} className="flex flex-1 flex-col items-center gap-1">
                  <div
                    className="w-full rounded-t bg-accent"
                    style={{ height: `${Math.max(8, (p.value / maxVal) * 100)}%` }}
                    title={`${p.date}: ${p.value}`}
                  />
                  <span className="text-[10px] text-ink-3">{p.date.slice(5)}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="overflow-x-auto rounded-xl border border-line bg-surface">
          <table className="w-full text-left text-sm">
            <thead className="border-b border-line bg-raised text-xs uppercase text-ink-3">
              <tr>
                <th className="px-4 py-3">Conversation</th>
                <th className="px-4 py-3">Contact</th>
                <th className="px-4 py-3">Inbox</th>
                <th className="px-4 py-3">Confidence</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {convs.length === 0 ? (
                <tr>
                  <td colSpan={4} className="px-4 py-6 text-center text-ink-3">No matched conversations yet.</td>
                </tr>
              ) : (
                convs.map((c) => (
                  <tr key={c.id} className="hover:bg-raised/40">
                    <td className="px-4 py-3">
                      <Link to={`/accounts/${accountId}/conversations/${c.id}`} className="font-mono text-accent hover:underline">
                        #{c.id}
                      </Link>
                    </td>
                    <td className="px-4 py-3 text-ink">{c.contact.name}</td>
                    <td className="px-4 py-3 text-ink-2">{c.inbox.name}</td>
                    <td className="px-4 py-3 font-mono text-xs text-ink-2">
                      {c.score != null ? `${Math.round(c.score * 100)}%` : "—"}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
