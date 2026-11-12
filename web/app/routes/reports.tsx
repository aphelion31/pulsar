import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { api } from "~/lib/api";
import type { ShellCtx } from "./shell";

type Point = { timestamp: number; value: number };
const metrics = [
  ["conversations_count", "Conversations"],
  ["incoming_messages_count", "Incoming messages"],
  ["outgoing_messages_count", "Outgoing messages"],
  ["avg_first_response_time", "First response time"],
  ["avg_resolution_time", "Resolution time"],
  ["resolutions_count", "Resolutions"],
  ["reply_time", "Reply time"],
] as const;

function dateInput(date: Date) {
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 10);
}

function Chart({ points, label }: { points: Point[]; label: string }) {
  if (!points.length) return <div className="flex h-40 items-center justify-center text-sm text-slate-500">No data available</div>;
  const max = Math.max(1, ...points.map(point => point.value));
  const coordinates = points.map((point, index) => `${points.length === 1 ? 50 : index * 100 / (points.length - 1)},${95 - point.value / max * 85}`).join(" ");
  return <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="h-40 w-full" role="img" aria-label={`${label} chart`}><polyline points={coordinates} fill="none" stroke="currentColor" strokeWidth="1.5" vectorEffect="non-scaling-stroke" className="text-blue-600" />{points.map((point, index) => <circle key={`${point.timestamp}-${index}`} cx={points.length === 1 ? 50 : index * 100 / (points.length - 1)} cy={95 - point.value / max * 85} r="1.5" className="fill-blue-600"><title>{new Date(point.timestamp * 1000).toLocaleDateString()}: {point.value}</title></circle>)}</svg>;
}

export default function Reports() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [from, setFrom] = useState(() => dateInput(new Date(Date.now() - 29 * 86400000)));
  const [to, setTo] = useState(() => dateInput(new Date()));
  const [group, setGroup] = useState("day");
  const [summary, setSummary] = useState<Record<string, number>>({});
  const [series, setSeries] = useState<Record<string, Point[]>>({});
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let active = true;
    const start = new Date(`${from}T00:00:00`).getTime();
    const end = new Date(`${to}T23:59:59`).getTime();
    if (!Number.isFinite(start) || !Number.isFinite(end) || start > end) { setError("Select a valid date range"); setLoading(false); return; }
    const query = new URLSearchParams({ since: String(Math.floor(start / 1000)), until: String(Math.floor(end / 1000)), group_by: group, type: "account" });
    const base = `/api/v2/accounts/${accountId}`;
    setLoading(true); setError("");
    Promise.all([api<Record<string, number>>(`${base}/reports/summary?${query}`), ...metrics.map(([key]) => api<Point[]>(`${base}/reports?${query}&metric=${key}`))]).then(([totals, ...charts]) => {
      if (!active) return;
      setSummary(totals as Record<string, number>);
      setSeries(Object.fromEntries(metrics.map(([key], i) => [key, charts[i] as Point[]])));
    }).catch((err: unknown) => { if (active) setError(err instanceof Error ? err.message : "Failed to load reports"); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [accountId, from, to, group]);
  return <div className="h-full overflow-auto p-6"><header className="mb-6"><h1 className="text-xl font-semibold">Conversation Reports</h1></header><div className="mb-6 flex flex-wrap items-end gap-4"><label className="grid gap-1 text-sm">From<input className="rounded border border-slate-300 bg-transparent p-2" type="date" value={from} onChange={event => setFrom(event.target.value)} /></label><label className="grid gap-1 text-sm">To<input className="rounded border border-slate-300 bg-transparent p-2" type="date" value={to} onChange={event => setTo(event.target.value)} /></label><label className="grid gap-1 text-sm">Group by<select className="rounded border border-slate-300 bg-transparent p-2" value={group} onChange={event => setGroup(event.target.value)}><option value="hour">Hour</option><option value="day">Day</option><option value="week">Week</option><option value="month">Month</option><option value="year">Year</option></select></label></div>{error && <p role="alert" className="mb-4 text-red-700">{error}</p>}{loading ? <p>Loading reports…</p> : !error && <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">{metrics.map(([key, label]) => <section key={key} className="rounded-lg border border-slate-200 p-5"><h2 className="text-sm font-medium">{label}</h2><p className="my-3 text-2xl font-semibold">{Number(summary[key] ?? 0).toLocaleString(undefined, { maximumFractionDigits: 2 })}{key.includes("time") ? " s" : ""}</p><Chart points={series[key] ?? []} label={label} /></section>)}</div>}</div>;
}
