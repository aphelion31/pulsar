import { useEffect, useState } from "react";
import { useLocation, useOutletContext, useParams } from "react-router";
import { BarChart } from "~/components/next/charts/BarChart";
import Bot from "~/components/next/reports/Bot";
import Csat from "~/components/next/reports/Csat";
import EntityReport from "~/components/next/reports/EntityReport";
import Live from "~/components/next/reports/Live";
import { api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "./shell";

type Point = { timestamp: number; value: number };
const metrics = [
  ["conversations_count", "CONVERSATIONS"],
  ["incoming_messages_count", "INCOMING_MESSAGES"],
  ["outgoing_messages_count", "OUTGOING_MESSAGES"],
  ["avg_first_response_time", "FIRST_RESPONSE_TIME"],
  ["avg_resolution_time", "RESOLUTION_TIME"],
  ["resolutions_count", "RESOLUTION_COUNT"],
  ["reply_time", "REPLY_TIME"],
] as const;

const RANGES = {
  LAST_7_DAYS: { offset: 6, groups: ["day"] },
  LAST_30_DAYS: { offset: 29, groups: ["day", "week"] },
  LAST_3_MONTHS: { offset: 89, groups: ["day", "week", "month"] },
  LAST_6_MONTHS: { offset: 179, groups: ["week", "month"] },
  LAST_YEAR: { offset: 364, groups: ["week", "month"] },
  CUSTOM_DATE_RANGE: { offset: null, groups: ["day", "week", "month", "year"] },
} as const;
type RangeId = keyof typeof RANGES;

export function formatTime(s: number) {
  if (s >= 60 && s < 3600) {
    const m = Math.floor(s / 60), sec = m === 60 ? 0 : Math.floor(s % 60);
    return `${m} Min${sec > 0 ? ` ${sec} Sec` : ""}`;
  }
  if (s >= 3600 && s < 86400) {
    const h = Math.floor(s / 3600), m = s % 3600 < 60 || h === 24 ? 0 : Math.floor((s % 3600) / 60);
    return `${h} Hr${m > 0 ? ` ${m} Min` : ""}`;
  }
  if (s >= 86400) {
    const d = Math.floor(s / 86400), h = s % 86400 < 3600 || d >= 364 ? 0 : Math.floor((s % 86400) / 3600);
    return `${d} Day${h > 0 ? ` ${h} Hr` : ""}`;
  }
  return `${Math.floor(s)} Sec`;
}

function dateInput(date: Date) {
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 10);
}

export default function Reports() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { pathname } = useLocation();
  const { id } = useParams();
  if (pathname.endsWith("/overview")) return <Live accountId={accountId} />;
  const section = pathname.split("/reports/")[1]?.split("/")[0] ?? "";
  if (section === "csat") return <Csat accountId={accountId} />;
  if (section === "bot") return <Bot accountId={accountId} />;
  const kind = section.startsWith("agent")
    ? "agent"
    : section.startsWith("inbox")
      ? "inbox"
      : section.startsWith("label")
        ? "label"
        : section.startsWith("team")
          ? "team"
          : undefined;
  if (kind) return <EntityReport accountId={accountId} kind={kind} id={id} />;
  return <ConversationReport accountId={accountId} />;
}

function ConversationReport({ accountId }: { accountId: number }) {
  const [range, setRange] = useState<RangeId>("LAST_7_DAYS");
  const [from, setFrom] = useState(() => dateInput(new Date(Date.now() - 29 * 86400000)));
  const [to, setTo] = useState(() => dateInput(new Date()));
  const [group, setGroup] = useState("day");
  const [businessHours, setBusinessHours] = useState(false);
  const [selectedMetric, setSelectedMetric] = useState<string>(metrics[0][0]);
  const [summary, setSummary] = useState<Record<string, number>>({});
  const [series, setSeries] = useState<Record<string, Point[]>>({});
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const offset = RANGES[range].offset;
  const groups = RANGES[range].groups as readonly string[];
  const pickRange = (id: RangeId) => {
    setRange(id);
    setGroup(RANGES[id].groups[0]);
  };

  useEffect(() => {
    let active = true;
    const today = dateInput(new Date());
    const start = new Date(`${offset === null ? from : dateInput(new Date(Date.now() - offset * 86400000))}T00:00:00`).getTime();
    const end = new Date(`${offset === null ? to : today}T23:59:59`).getTime();
    if (!Number.isFinite(start) || !Number.isFinite(end) || start > end) {
      setError("Select a valid date range");
      setLoading(false);
      return;
    }
    const query = new URLSearchParams({
      since: String(Math.floor(start / 1000)),
      until: String(Math.floor(end / 1000)),
      group_by: group,
      type: "account",
      business_hours: String(businessHours),
      timezone_offset: String(-new Date().getTimezoneOffset() / 60),
    });
    const base = `/api/v2/accounts/${accountId}`;
    setLoading(true);
    setError("");
    Promise.all([
      api<Record<string, number>>(`${base}/reports/summary?${query}`),
      ...metrics.map(([key]) => api<Point[]>(`${base}/reports?${query}&metric=${key}`)),
    ])
      .then(([totals, ...charts]) => {
        if (!active) return;
        setSummary(totals as Record<string, number>);
        setSeries(Object.fromEntries(metrics.map(([key], i) => [key, charts[i] as Point[]])));
      })
      .catch((err: unknown) => {
        if (active) setError(err instanceof Error ? err.message : "Failed to load reports");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [accountId, from, to, group, offset, businessHours]);

  const field = "h-9 rounded-lg border border-n-weak bg-n-solid-1 px-3 text-sm text-n-slate-12 focus:border-n-brand focus:outline-none";
  const activeMeta = metrics.find(([k]) => k === selectedMetric) ?? metrics[0];
  const activePoints = series[activeMeta[0]] ?? [];

  return (
    <div className="h-full overflow-auto bg-n-background p-6">
      <div className="mx-auto max-w-6xl space-y-5">
        <header className="flex items-center justify-between">
          <h1 className="font-inter-display text-xl font-semibold text-n-slate-12">
            {t("REPORT.HEADER")}
          </h1>
        </header>

        <div className="flex flex-wrap items-end gap-4 rounded-xl border border-n-weak bg-n-solid-2 p-4">
          <label className="grid gap-1 text-xs font-medium text-n-slate-11">
            {t("REPORT.DURATION_FILTER_LABEL")}
            <select className={field} value={range} onChange={(e) => pickRange(e.target.value as RangeId)}>
              {(Object.keys(RANGES) as RangeId[]).map((id) => (
                <option key={id} value={id}>
                  {t(`REPORT.DATE_RANGE_OPTIONS.${id}`)}
                </option>
              ))}
            </select>
          </label>
          {offset === null && (
            <>
              <label className="grid gap-1 text-xs font-medium text-n-slate-11">
                {t("REPORT.CUSTOM_DATE_RANGE.PLACEHOLDER")}
                <input className={field} type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
              </label>
              <label className="grid gap-1 text-xs font-medium text-n-slate-11">
                <span>To</span>
                <input className={field} type="date" value={to} onChange={(e) => setTo(e.target.value)} />
              </label>
            </>
          )}
          <label className="grid gap-1 text-xs font-medium text-n-slate-11">
            {t("REPORT.GROUP_BY_FILTER_DROPDOWN_LABEL")}
            <select className={field} value={group} onChange={(e) => setGroup(e.target.value)}>
              {groups.map((g) => (
                <option key={g} value={g}>
                  {t(`REPORT.GROUPING_OPTIONS.${g.toUpperCase()}`)}
                </option>
              ))}
            </select>
          </label>
          <label className="flex h-9 items-center gap-2 text-sm text-n-slate-12">
            <input
              type="checkbox"
              checked={businessHours}
              onChange={(e) => setBusinessHours(e.target.checked)}
              className="accent-n-brand"
            />
            {t("REPORT.BUSINESS_HOURS")}
          </label>
        </div>

        {error && (
          <p role="alert" className="rounded-lg bg-n-ruby-3 px-3 py-2 text-sm text-n-ruby-11">
            {error}
          </p>
        )}

        {loading ? (
          <div className="rounded-xl border border-n-weak bg-n-solid-2 p-8 text-sm text-n-slate-10">
            {t("REPORT.LOADING_CHART")}
          </div>
        ) : !error && (
          <div className="space-y-4">
            {/* Metric Selector Strip (ReportContainer.vue ChartStats tabs) */}
            <div className="grid grid-cols-2 gap-px overflow-hidden rounded-xl border border-n-weak bg-n-weak sm:grid-cols-4 lg:grid-cols-7">
              {metrics.map(([key, label]) => {
                const active = selectedMetric === key;
                const val = key.includes("time")
                  ? formatTime(Number(summary[key] ?? 0))
                  : Number(summary[key] ?? 0).toLocaleString();
                return (
                  <button
                    key={key}
                    type="button"
                    onClick={() => setSelectedMetric(key)}
                    className={`flex flex-col justify-between p-4 text-left transition ${
                      active
                        ? "bg-n-solid-1 ring-2 ring-inset ring-n-brand"
                        : "bg-n-solid-2 hover:bg-n-solid-1"
                    }`}
                  >
                    <span className="text-xs font-medium text-n-slate-11">
                      {t(`REPORT.METRICS.${label}.NAME`)}
                    </span>
                    <span className="mt-2 font-inter-display text-2xl font-semibold tabular-nums text-n-slate-12">
                      {val}
                    </span>
                  </button>
                );
              })}
            </div>

            {/* Active Bar Chart */}
            <section className="rounded-xl border border-n-weak bg-n-solid-1 p-5 shadow-xs">
              <div className="mb-4">
                <h2 className="text-base font-medium text-n-slate-12">
                  {t(`REPORT.METRICS.${activeMeta[1]}.NAME`)}
                </h2>
                <p className="text-xs text-n-slate-11">
                  {t(`REPORT.METRICS.${activeMeta[1]}.DESC`)}
                </p>
              </div>
              <BarChart
                height={300}
                ariaLabel={t(`REPORT.METRICS.${activeMeta[1]}.NAME`)}
                data={{
                  categories: activePoints.map((pt) =>
                    new Date(pt.timestamp * 1000).toLocaleDateString(undefined, {
                      month: "short",
                      day: "numeric",
                    })
                  ),
                  series: [
                    {
                      label: t(`REPORT.METRICS.${activeMeta[1]}.NAME`),
                      color: "#2781F6",
                      data: activePoints.map((pt) => pt.value),
                    },
                  ],
                }}
              />
            </section>
          </div>
        )}
      </div>
    </div>
  );
}
