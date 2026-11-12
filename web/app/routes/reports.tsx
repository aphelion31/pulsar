import { useEffect, useState } from "react";
import { useLocation, useOutletContext, useParams } from "react-router";
import Live from "~/components/next/reports/Live";
import EntityReport from "~/components/next/reports/EntityReport";
import Csat from "~/components/next/reports/Csat";
import Bot from "~/components/next/reports/Bot";
import { api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "./shell";

type Point = { timestamp: number; value: number };
// mirrors chatwoot ReportContainer.vue metric keys + REPORT.METRICS labels
const metrics = [
  ["conversations_count", "CONVERSATIONS"],
  ["incoming_messages_count", "INCOMING_MESSAGES"],
  ["outgoing_messages_count", "OUTGOING_MESSAGES"],
  ["avg_first_response_time", "FIRST_RESPONSE_TIME"],
  ["avg_resolution_time", "RESOLUTION_TIME"],
  ["resolutions_count", "RESOLUTION_COUNT"],
  ["reply_time", "REPLY_TIME"],
] as const;

// mirrors chatwoot reports/constants.js DATE_RANGE_OPTIONS
const RANGES = {
  LAST_7_DAYS: { offset: 6, groups: ["day"] },
  LAST_30_DAYS: { offset: 29, groups: ["day", "week"] },
  LAST_3_MONTHS: { offset: 89, groups: ["day", "week", "month"] },
  LAST_6_MONTHS: { offset: 179, groups: ["week", "month"] },
  LAST_YEAR: { offset: 364, groups: ["week", "month"] },
  CUSTOM_DATE_RANGE: { offset: null, groups: ["day", "week", "month", "year"] },
} as const;
type RangeId = keyof typeof RANGES;

// port of @chatwoot/utils formatTime
export function formatTime(s: number) {
  if (s >= 60 && s < 3600) { const m = Math.floor(s / 60), sec = m === 60 ? 0 : Math.floor(s % 60); return `${m} Min${sec > 0 ? ` ${sec} Sec` : ""}`; }
  if (s >= 3600 && s < 86400) { const h = Math.floor(s / 3600), m = s % 3600 < 60 || h === 24 ? 0 : Math.floor((s % 3600) / 60); return `${h} Hr${m > 0 ? ` ${m} Min` : ""}`; }
  if (s >= 86400) { const d = Math.floor(s / 86400), h = s % 86400 < 3600 || d >= 364 ? 0 : Math.floor((s % 86400) / 3600); return `${d} Day${h > 0 ? ` ${h} Hr` : ""}`; }
  return `${Math.floor(s)} Sec`;
}

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
  const { pathname } = useLocation();
  const { id } = useParams();
  if (pathname.endsWith("/overview")) return <Live accountId={accountId} />;
  const section = pathname.split("/reports/")[1]?.split("/")[0] ?? "";
  if (section === "csat") return <Csat accountId={accountId} />;
  if (section === "bot") return <Bot accountId={accountId} />;
  const kind = section.startsWith("agent") ? "agent" : section.startsWith("inbox") ? "inbox" : section.startsWith("label") ? "label" : section.startsWith("team") ? "team" : undefined;
  if (kind) return <EntityReport accountId={accountId} kind={kind} id={id} />;
  return <ConversationReport accountId={accountId} />;
}

function ConversationReport({ accountId }: { accountId: number }) {
  const [range, setRange] = useState<RangeId>("LAST_7_DAYS");
  const [from, setFrom] = useState(() => dateInput(new Date(Date.now() - 29 * 86400000)));
  const [to, setTo] = useState(() => dateInput(new Date()));
  const [group, setGroup] = useState("day");
  const [businessHours, setBusinessHours] = useState(false);
  const [summary, setSummary] = useState<Record<string, number>>({});
  const [series, setSeries] = useState<Record<string, Point[]>>({});
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const offset = RANGES[range].offset;
  const groups = RANGES[range].groups as readonly string[];
  const pickRange = (id: RangeId) => { setRange(id); setGroup(RANGES[id].groups[0]); };
  useEffect(() => {
    let active = true;
    const today = dateInput(new Date());
    const start = new Date(`${offset === null ? from : dateInput(new Date(Date.now() - offset * 86400000))}T00:00:00`).getTime();
    const end = new Date(`${offset === null ? to : today}T23:59:59`).getTime();
    if (!Number.isFinite(start) || !Number.isFinite(end) || start > end) { setError("Select a valid date range"); setLoading(false); return; }
    const query = new URLSearchParams({ since: String(Math.floor(start / 1000)), until: String(Math.floor(end / 1000)), group_by: group, type: "account", business_hours: String(businessHours), timezone_offset: String(-new Date().getTimezoneOffset() / 60) });
    const base = `/api/v2/accounts/${accountId}`;
    setLoading(true); setError("");
    Promise.all([api<Record<string, number>>(`${base}/reports/summary?${query}`), ...metrics.map(([key]) => api<Point[]>(`${base}/reports?${query}&metric=${key}`))]).then(([totals, ...charts]) => {
      if (!active) return;
      setSummary(totals as Record<string, number>);
      setSeries(Object.fromEntries(metrics.map(([key], i) => [key, charts[i] as Point[]])));
    }).catch((err: unknown) => { if (active) setError(err instanceof Error ? err.message : "Failed to load reports"); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [accountId, from, to, group, offset, businessHours]);
  const field = "rounded-lg border border-n-weak bg-n-background p-2 text-sm";
  return <div className="h-full overflow-auto p-6"><header className="mb-6 flex items-center justify-between"><h1 className="text-xl font-medium">{t("REPORT.HEADER")}</h1></header>
    <div className="mb-6 flex flex-wrap items-end gap-4">
      <label className="grid gap-1 text-sm">{t("REPORT.DURATION_FILTER_LABEL")}<select className={field} value={range} onChange={e => pickRange(e.target.value as RangeId)}>{(Object.keys(RANGES) as RangeId[]).map(id => <option key={id} value={id}>{t(`REPORT.DATE_RANGE_OPTIONS.${id}`)}</option>)}</select></label>
      {offset === null && <><label className="grid gap-1 text-sm">{t("REPORT.CUSTOM_DATE_RANGE.PLACEHOLDER")}<input className={field} type="date" value={from} onChange={e => setFrom(e.target.value)} /></label><label className="grid gap-1 text-sm"><span className="sr-only">To</span><input className={field} type="date" value={to} onChange={e => setTo(e.target.value)} /></label></>}
      <label className="grid gap-1 text-sm">{t("REPORT.GROUP_BY_FILTER_DROPDOWN_LABEL")}<select className={field} value={group} onChange={e => setGroup(e.target.value)}>{groups.map(g => <option key={g} value={g}>{t(`REPORT.GROUPING_OPTIONS.${g.toUpperCase()}`)}</option>)}</select></label>
      <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={businessHours} onChange={e => setBusinessHours(e.target.checked)} />{t("REPORT.BUSINESS_HOURS")}</label>
    </div>
    {error && <p role="alert" className="mb-4 text-n-ruby-11">{error}</p>}
    {loading ? <p>{t("REPORT.LOADING_CHART")}</p> : !error && <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">{metrics.map(([key, label]) => { const name = t(`REPORT.METRICS.${label}.NAME`); return <section key={key} className="rounded-lg border border-n-weak p-5"><h2 className="text-sm font-medium">{name} <span className="text-n-slate-11">{t(`REPORT.METRICS.${label}.DESC`)}</span></h2><p className="my-3 text-2xl font-semibold">{key.includes("time") ? formatTime(Number(summary[key] ?? 0)) : Number(summary[key] ?? 0).toLocaleString()}</p><Chart points={series[key] ?? []} label={name} /></section>; })}</div>}</div>;
}
