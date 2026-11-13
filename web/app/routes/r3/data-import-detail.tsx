import { useEffect, useRef, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import { Button } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";
import { formatDate, formatStatus, isActive, POLL_MS, statusDot } from "./data-import-utils";
import type { DataImport } from "./data-imports";

type Log = { id: number; kind?: string | null; error_code: string; message: string | null; source_object_type: string | null; source_object_id: string | null; created_at: string };
type Detail = DataImport & {
  processing_errors: string | null; import_errors_count: number; skip_logs_count: number; import_errors: Log[]; skip_logs: Log[];
  skip_logs_filters: { selected_source_object_type: string | null; counts_by_type: Record<string, number> };
  initiated_by: { id: number; name: string; email: string } | null; started_at: string | null; completed_at: string | null;
  stats: { contacts?: { imported?: number; skipped?: number; total?: number }; errors?: { count?: number } };
};

const PREFIX = "DATA_IMPORTS.";
const SKIP_TYPES = ["contact", "conversation", "message"];

export default function DataImportDetail() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { dataImportId } = useParams();
  const [item, setItem] = useState<Detail | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [skipType, setSkipType] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const base = `${acct(accountId)}/data_imports/${dataImportId}`;
  const cancelled = useRef(false);

  async function load(type: string | null = skipType) {
    const q = type ? `?skip_logs_type=${encodeURIComponent(type)}` : "";
    const r = await api<Detail>(`${base}${q}`);
    if (!cancelled.current) setItem(r);
  }

  useEffect(() => {
    cancelled.current = false;
    setError("");
    load().catch((e) => { if (!cancelled.current) setError(e instanceof Error ? e.message : "Request failed"); });
    return () => { cancelled.current = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [base]);

  // Poll while pending or processing.
  useEffect(() => {
    if (!item || !isActive(item.status)) return;
    const id = window.setInterval(() => { if (!document.hidden) load().catch(() => {}); }, POLL_MS);
    return () => window.clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item?.status, base, skipType]);

  async function action(path: string, success: string) {
    setBusy(true); setError(""); setNotice("");
    try {
      const r = await api<Detail>(`${base}${path}`, { method: "POST" });
      setItem(r); setNotice(success);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed");
    } finally { setBusy(false); }
  }

  async function download(kind: "error_logs" | "skip_logs") {
    const res = await fetch(`${base}/${kind}`, { headers: authHeaders() });
    if (!res.ok) { setError("Request failed"); return; }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = `data-import-${dataImportId}-${kind.replace("_", "-")}.csv`; a.click();
    URL.revokeObjectURL(url);
  }

  if (error && !item) {
    return <div className="p-6 text-[13px] text-red-600" role="alert">{error}</div>;
  }
  if (!item) return <div className="p-6 text-[13px] text-ink-3">{t(`${PREFIX}LOADING`)}</div>;

  const imported = Number(item.processed_records || 0);
  const total = item.total_records;
  const progressLabel = total != null
    ? t(`${PREFIX}DETAIL.PROGRESS_WITH_TOTAL`, { imported, total })
    : t(`${PREFIX}DETAIL.PROGRESS_WITHOUT_TOTAL`, { imported });
  const pct = total ? Math.min(100, Math.round((imported / total) * 100)) : 0;
  const canRetry = item.status === "failed" || item.status === "abandoned";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="border-b border-line px-6 py-4">
        <Link to={`/accounts/${accountId}/settings/data`} className="text-[13px] text-accent">← {t(`${PREFIX}DETAIL.BACK`)}</Link>
        <div className="mt-2 flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-[15px] font-medium">{item.name || t(`${PREFIX}TABLE.UNNAMED`)}</h1>
            <div className="mt-1 flex items-center gap-2 text-[13px] text-ink-3">
              <span className={`size-2 rounded-full ${statusDot(item.status)} ${isActive(item.status) ? "animate-pulse" : ""}`} />
              <span className="capitalize">{formatStatus(item.status)}</span>
              {isActive(item.status) && <span className="hidden sm:inline">· {t(`${PREFIX}MONITOR.LIVE`, { seconds: POLL_MS / 1000 })}</span>}
            </div>
          </div>
          <div className="flex gap-2">
            {canRetry && <Button onClick={() => action("/retry", t(`${PREFIX}ALERTS.IMPORT_RETRIED`))} disabled={busy}>{t(`${PREFIX}TABLE.RETRY`)}</Button>}
            {isActive(item.status) && <Button variant="danger" onClick={() => action("/abandon", t(`${PREFIX}ALERTS.IMPORT_ABANDONED`))} disabled={busy}>{t(`${PREFIX}TABLE.ABANDON`)}</Button>}
          </div>
        </div>
      </header>

      <div className="min-h-0 flex-1 space-y-6 overflow-y-auto p-6">
        {error && <p role="alert" className="text-[13px] text-red-600">{error}</p>}
        {notice && <p role="status" className="text-[13px] text-teal-700">{notice}</p>}
        {item.processing_errors && <p role="alert" className="text-[13px] text-red-600">{item.processing_errors}</p>}

        <section aria-label={t(`${PREFIX}DETAIL.PROGRESS`)} className="rounded-xl border border-line bg-surface p-5">
          <h2 className="text-[13px] font-medium text-ink-2">{t(`${PREFIX}DETAIL.PROGRESS`)}</h2>
          <p className="mt-1 text-[14px] tabular-nums">{progressLabel}</p>
          {total ? <div className="mt-3 h-2 overflow-hidden rounded-full bg-bg"><div className="h-full bg-accent transition-all" style={{ width: `${pct}%` }} /></div> : null}
          <dl className="mt-4 grid grid-cols-2 gap-x-6 gap-y-3 text-[13px] sm:grid-cols-4">
            <Tile label={t(`${PREFIX}DETAIL.SOURCE`)} value="CSV" />
            <Tile label={t(`${PREFIX}DETAIL.IMPORT_TYPES`)} value={t(`${PREFIX}TYPES.CONTACTS`)} />
            <Tile label={t(`${PREFIX}DETAIL.CREATED`)} value={formatDate(item.created_at)} />
            <Tile label={t(`${PREFIX}DETAIL.INITIATED_BY`)} value={item.initiated_by?.name || item.initiated_by?.email || "-"} />
          </dl>
        </section>

        <section aria-label={t(`${PREFIX}DETAIL.ERRORS`)} className="rounded-xl border border-line bg-surface p-5">
          <div className="flex items-center justify-between">
            <h2 className="text-[13px] font-medium text-ink-2">{t(`${PREFIX}DETAIL.ERRORS`)}</h2>
            {item.import_errors_count > 0 && <Button variant="quiet" onClick={() => download("error_logs")}>{t(`${PREFIX}DETAIL.DOWNLOAD_ERROR_LOGS`)}</Button>}
          </div>
          {item.import_errors.length === 0 ? <p className="mt-2 text-[13px] text-ink-3">{t(`${PREFIX}DETAIL.NO_ERRORS`)}</p> : <LogTable logs={item.import_errors} />}
        </section>

        <section aria-label={t(`${PREFIX}DETAIL.SKIP_LOGS`)} className="rounded-xl border border-line bg-surface p-5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="text-[13px] font-medium text-ink-2">{t(`${PREFIX}DETAIL.SKIP_LOGS`)}</h2>
            <div className="flex items-center gap-2">
              <select aria-label={t(`${PREFIX}DETAIL.SKIP_LOGS`)} className="rounded-lg border border-line bg-bg px-2 py-1 text-[13px]" value={skipType ?? ""}
                onChange={(e) => { const v = e.target.value || null; setSkipType(v); load(v).catch(() => {}); }}>
                <option value="">{t(`${PREFIX}DETAIL.ALL_SKIP_LOGS`)}</option>
                {SKIP_TYPES.map((type) => <option key={type} value={type}>{type} ({item.skip_logs_filters.counts_by_type[type] ?? 0})</option>)}
              </select>
              {item.skip_logs_count > 0 && <Button variant="quiet" onClick={() => download("skip_logs")}>{t(`${PREFIX}DETAIL.DOWNLOAD_SKIP_LOGS`)}</Button>}
            </div>
          </div>
          {item.skip_logs.length === 0 ? <p className="mt-2 text-[13px] text-ink-3">{t(`${PREFIX}DETAIL.NO_SKIP_LOGS`)}</p> : <LogTable logs={item.skip_logs} showKind />}
        </section>
      </div>
    </div>
  );
}

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-ink-3">{label}</dt>
      <dd className="mt-0.5 text-[14px] text-ink">{value}</dd>
    </div>
  );
}

function LogTable({ logs, showKind }: { logs: Log[]; showKind?: boolean }) {
  return (
    <div className="mt-3 overflow-x-auto">
      <table className="w-full text-left text-[13px]">
        <thead className="text-ink-3">
          <tr>
            {showKind && <th className="py-2 pr-4 font-medium">{t(`${PREFIX}DETAIL.KIND`)}</th>}
            <th className="py-2 pr-4 font-medium">{t(`${PREFIX}DETAIL.ERROR_CODE`)}</th>
            <th className="py-2 pr-4 font-medium">{t(`${PREFIX}DETAIL.SOURCE_OBJECT`)}</th>
            <th className="py-2 pr-4 font-medium">{t(`${PREFIX}DETAIL.MESSAGE`)}</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {logs.map((log) => (
            <tr key={log.id}>
              {showKind && <td className="py-2 pr-4 capitalize">{log.kind ?? "-"}</td>}
              <td className="py-2 pr-4 font-mono text-[12px]">{log.error_code}</td>
              <td className="py-2 pr-4">{[log.source_object_type, log.source_object_id].filter(Boolean).join(": ") || "-"}</td>
              <td className="py-2 pr-4">{log.message || "-"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function authHeaders(): HeadersInit {
  const raw = localStorage.getItem("pulsar.session");
  const s = raw ? JSON.parse(raw) : null;
  return s ? { "access-token": s["access-token"], client: s.client, uid: s.uid } : {};
}
