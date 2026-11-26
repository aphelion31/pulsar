import { useEffect, useState } from "react";
import { Link, useLocation, useOutletContext, useParams } from "react-router";
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
  const [drillPage, setDrillPage] = useState(1);
  const [drillData, setDrillData] = useState<{
    meta: { total_count: number; conversation_count: number; record_type: string; current_page: number; per_page: number };
    payload: Array<{
      record_type: string;
      occurred_at: number | null;
      metric_value: number | null;
      conversation?: { id: number; display_id: number; contact_name?: string | null; inbox_name?: string | null; assignee_name?: string | null; status: string };
      message?: { id: number; content?: string | null; sender_name?: string | null } | null;
    }>;
  } | null>(null);
  const [breakdownTab, setBreakdownTab] = useState<"matrix" | "frt" | "outgoing">("matrix");
  const [matrixData, setMatrixData] = useState<{
    inboxes: Array<{ id: number; name: string }>;
    labels: Array<{ id: number; title: string }>;
    matrix: number[][];
  } | null>(null);
  const [frtData, setFrtData] = useState<Record<string, Record<string, number>>>({});
  const [outgoingGroup, setOutgoingGroup] = useState<"agent" | "inbox" | "team" | "label">("agent");
  const [outgoingRows, setOutgoingRows] = useState<Array<{ id: number | null; name: string; outgoing_messages_count: number }>>([]);
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

  useEffect(() => {
    setDrillPage(1);
  }, [selectedMetric, from, to, offset]);

  useEffect(() => {
    let active = true;
    const today = dateInput(new Date());
    const start = new Date(`${offset === null ? from : dateInput(new Date(Date.now() - offset * 86400000))}T00:00:00`).getTime();
    const end = new Date(`${offset === null ? to : today}T23:59:59`).getTime();
    if (!Number.isFinite(start) || !Number.isFinite(end) || start > end) return;
    const since = String(Math.floor(start / 1000));
    const until = String(Math.floor(end / 1000));
    const base = `/api/v2/accounts/${accountId}`;

    api<{
      meta: { total_count: number; conversation_count: number; record_type: string; current_page: number; per_page: number };
      payload: Array<{
        record_type: string;
        occurred_at: number | null;
        metric_value: number | null;
        conversation?: { id: number; display_id: number; contact_name?: string | null; inbox_name?: string | null; assignee_name?: string | null; status: string };
        message?: { id: number; content?: string | null; sender_name?: string | null } | null;
      }>;
    }>(`${base}/reports/drilldown?metric=${encodeURIComponent(selectedMetric)}&since=${since}&until=${until}&type=account&business_hours=${businessHours}&page=${drillPage}&per_page=10`)
      .then((res) => { if (active) setDrillData(res); })
      .catch(() => { if (active) setDrillData(null); });

    if (breakdownTab === "matrix") {
      api<{ inboxes: Array<{ id: number; name: string }>; labels: Array<{ id: number; title: string }>; matrix: number[][] }>(
        `${base}/reports/inbox_label_matrix?since=${since}&until=${until}`
      )
        .then((res) => { if (active) setMatrixData(res); })
        .catch(() => { if (active) setMatrixData(null); });
    } else if (breakdownTab === "frt") {
      api<Record<string, Record<string, number>>>(
        `${base}/reports/first_response_time_distribution?since=${since}&until=${until}`
      )
        .then((res) => { if (active) setFrtData(res || {}); })
        .catch(() => { if (active) setFrtData({}); });
    } else {
      api<Array<{ id: number | null; name: string; outgoing_messages_count: number }>>(
        `${base}/reports/outgoing_messages_count?since=${since}&until=${until}&group_by=${outgoingGroup}`
      )
        .then((res) => { if (active) setOutgoingRows(Array.isArray(res) ? res : []); })
        .catch(() => { if (active) setOutgoingRows([]); });
    }
    return () => { active = false; };
  }, [accountId, from, to, offset, businessHours, selectedMetric, drillPage, breakdownTab, outgoingGroup]);

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
          <Link
            to={`/accounts/${accountId}/reports/year-in-review`}
            className="inline-flex h-9 items-center rounded-lg border border-n-weak bg-n-solid-1 px-3 text-xs font-medium text-n-slate-12 transition hover:bg-n-alpha-1"
          >
            {t("YEAR_IN_REVIEW.TITLE")}
          </Link>
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

            {/* Drilldown Records Table */}
            <section className="rounded-xl border border-n-weak bg-n-solid-1 p-5 shadow-xs space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <h2 className="text-base font-medium text-n-slate-12">Metric Drilldown Records</h2>
                  <p className="text-xs text-n-slate-11">
                    {drillData
                      ? `${drillData.meta.total_count} records across ${drillData.meta.conversation_count} conversations`
                      : "Loading records..."}
                  </p>
                </div>
                {drillData && drillData.meta.total_count > drillData.meta.per_page && (
                  <div className="flex items-center gap-2 text-xs">
                    <button
                      type="button"
                      disabled={drillPage <= 1}
                      onClick={() => setDrillPage((p) => Math.max(1, p - 1))}
                      className="rounded border border-n-weak px-2.5 py-1 disabled:opacity-40"
                    >
                      Previous
                    </button>
                    <span className="text-n-slate-11">Page {drillPage}</span>
                    <button
                      type="button"
                      disabled={drillPage * drillData.meta.per_page >= drillData.meta.total_count}
                      onClick={() => setDrillPage((p) => p + 1)}
                      className="rounded border border-n-weak px-2.5 py-1 disabled:opacity-40"
                    >
                      Next
                    </button>
                  </div>
                )}
              </div>
              {drillData?.payload.length ? (
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-xs text-n-slate-12">
                    <thead className="border-b border-n-weak text-n-slate-10 uppercase">
                      <tr>
                        <th className="py-2 pr-3">Conversation</th>
                        <th className="py-2 pr-3">Contact</th>
                        <th className="py-2 pr-3">Inbox</th>
                        <th className="py-2 pr-3">Assignee</th>
                        <th className="py-2 pr-3">Detail</th>
                        <th className="py-2">Occurred</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-n-weak">
                      {drillData.payload.map((row, idx) => (
                        <tr key={`${row.conversation?.id ?? "c"}-${row.message?.id ?? idx}`}>
                          <td className="py-2 pr-3 font-medium">
                            {row.conversation ? (
                              <Link
                                to={`/accounts/${accountId}/conversations/${row.conversation.display_id}`}
                                className="text-n-brand hover:underline"
                              >
                                #{row.conversation.display_id} ({row.conversation.status})
                              </Link>
                            ) : "—"}
                          </td>
                          <td className="py-2 pr-3">{row.conversation?.contact_name || "—"}</td>
                          <td className="py-2 pr-3">{row.conversation?.inbox_name || "—"}</td>
                          <td className="py-2 pr-3">{row.conversation?.assignee_name || "Unassigned"}</td>
                          <td className="py-2 pr-3 max-w-xs truncate text-n-slate-11">
                            {row.message?.content ?? (row.metric_value !== null ? formatTime(row.metric_value) : "—")}
                          </td>
                          <td className="py-2 text-n-slate-10">
                            {row.occurred_at ? new Date(row.occurred_at * 1000).toLocaleString() : "—"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <p className="text-xs text-n-slate-10">No matching drilldown records in this date range.</p>
              )}
            </section>

            {/* Tabbed Report Breakdowns: Inbox x Label Matrix, FRT Distribution, Outgoing Volume */}
            <section className="rounded-xl border border-n-weak bg-n-solid-1 p-5 shadow-xs space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-3 border-b border-n-weak pb-3">
                <div className="flex flex-wrap gap-1">
                  {([
                    ["matrix", "Inbox × Label Matrix"],
                    ["frt", "FRT Distribution"],
                    ["outgoing", "Outgoing Messages"],
                  ] as const).map(([key, label]) => (
                    <button
                      key={key}
                      type="button"
                      onClick={() => setBreakdownTab(key)}
                      aria-pressed={breakdownTab === key}
                      className={`rounded-lg px-3 py-1.5 text-xs font-medium transition ${
                        breakdownTab === key
                          ? "bg-n-brand text-white"
                          : "bg-n-solid-2 text-n-slate-11 hover:text-n-slate-12"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>
                {breakdownTab === "outgoing" && (
                  <select
                    aria-label="Group outgoing messages by"
                    className="h-8 rounded-lg border border-n-weak bg-n-solid-2 px-2.5 text-xs text-n-slate-12"
                    value={outgoingGroup}
                    onChange={(e) => setOutgoingGroup(e.target.value as "agent" | "inbox" | "team" | "label")}
                  >
                    <option value="agent">By Agent</option>
                    <option value="inbox">By Inbox</option>
                    <option value="team">By Team</option>
                    <option value="label">By Label</option>
                  </select>
                )}
              </div>

              {breakdownTab === "matrix" && (
                matrixData && matrixData.inboxes.length > 0 && matrixData.labels.length > 0 ? (
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-xs text-n-slate-12">
                      <thead className="border-b border-n-weak text-n-slate-10">
                        <tr>
                          <th className="py-2 pr-4">Inbox</th>
                          {matrixData.labels.map((l) => (
                            <th key={l.id} className="py-2 px-2 text-right font-medium">{l.title}</th>
                          ))}
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-n-weak">
                        {matrixData.inboxes.map((inb, rIdx) => (
                          <tr key={inb.id}>
                            <td className="py-2 pr-4 font-medium">{inb.name}</td>
                            {matrixData.labels.map((l, cIdx) => (
                              <td key={l.id} className="py-2 px-2 text-right tabular-nums">
                                {matrixData.matrix[rIdx]?.[cIdx] ?? 0}
                              </td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ) : (
                  <p className="text-xs text-n-slate-10">Create inboxes and labels to view the matrix.</p>
                )
              )}

              {breakdownTab === "frt" && (
                Object.keys(frtData).length > 0 ? (
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-xs text-n-slate-12">
                      <thead className="border-b border-n-weak text-n-slate-10">
                        <tr>
                          <th className="py-2 pr-4">Channel</th>
                          <th className="py-2 px-2 text-right">0-1h</th>
                          <th className="py-2 px-2 text-right">1-4h</th>
                          <th className="py-2 px-2 text-right">4-8h</th>
                          <th className="py-2 px-2 text-right">8-24h</th>
                          <th className="py-2 px-2 text-right">24h+</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-n-weak">
                        {Object.entries(frtData).map(([channel, buckets]) => (
                          <tr key={channel}>
                            <td className="py-2 pr-4 font-medium">{channel}</td>
                            {(["0-1h", "1-4h", "4-8h", "8-24h", "24h+"] as const).map((b) => (
                              <td key={b} className="py-2 px-2 text-right tabular-nums">{buckets[b] ?? 0}</td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ) : (
                  <p className="text-xs text-n-slate-10">No first-response distribution events in this range.</p>
                )
              )}

              {breakdownTab === "outgoing" && (
                outgoingRows.length > 0 ? (
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-xs text-n-slate-12">
                      <thead className="border-b border-n-weak text-n-slate-10">
                        <tr>
                          <th className="py-2 pr-4">Name</th>
                          <th className="py-2 text-right">Outgoing Messages</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-n-weak">
                        {outgoingRows.map((r, i) => (
                          <tr key={`${r.id ?? r.name}-${i}`}>
                            <td className="py-2 pr-4 font-medium">{r.name}</td>
                            <td className="py-2 text-right tabular-nums">{r.outgoing_messages_count}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ) : (
                  <p className="text-xs text-n-slate-10">No outgoing messages recorded for this grouping.</p>
                )
              )}
            </section>
          </div>
        )}
      </div>
    </div>
  );
}
