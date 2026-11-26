import { useEffect, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import { Button } from "~/components/ui";
import { acct, api, getSession } from "~/lib/api";
import type { ShellCtx } from "../shell";
import { formatDate, formatStatus, statusDot } from "./data-import-utils";

type ExportDetail = {
  id: number;
  account_id: number;
  name: string;
  data_type: string;
  status: string;
  export_options: Record<string, unknown>;
  processed_records: number;
  total_records: number | null;
  error_message: string | null;
  initiated_by: { id: number; name: string } | null;
  file_url: string | null;
  started_at: string | number | null;
  completed_at: string | number | null;
  created_at: string | number;
  updated_at: string | number;
};

export default function DataExportDetail() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { dataExportId } = useParams();
  const base = `${acct(accountId)}/data_exports/${dataExportId}`;
  const [item, setItem] = useState<ExportDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  async function load() {
    setError("");
    const res = await api<ExportDetail>(base);
    setItem(res);
  }

  useEffect(() => {
    setLoading(true);
    load()
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load export"))
      .finally(() => setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [base]);

  async function handleRerun() {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const next = await api<ExportDetail>(`${base}/rerun`, { method: "POST" });
      setItem(next);
      setNotice("Export regenerated.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to rerun export");
    } finally {
      setBusy(false);
    }
  }

  async function handleDownload() {
    const sess = getSession();
    const headers: Record<string, string> = {};
    if (sess) {
      headers["access-token"] = sess["access-token"];
      headers["client"] = sess.client;
      headers["uid"] = sess.uid;
    }
    const res = await fetch(`${base}/download`, { headers });
    if (!res.ok) {
      setError(`Download failed (${res.status})`);
      return;
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `export-${dataExportId}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }

  function formatTs(v: string | number | null | undefined): string {
    if (v === null || v === undefined) return "—";
    if (typeof v === "number") return new Date(v * 1000).toLocaleString();
    return formatDate(v);
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-6 text-n-slate-12">
      <div className="mb-5 flex flex-wrap items-center justify-between gap-3 border-b border-line pb-4">
        <div className="flex items-center gap-3">
          <Link to={`/accounts/${accountId}/settings/data`} className="text-[13px] text-ink-2 hover:text-ink">
            ← Back to Data Management
          </Link>
          <span className="text-ink-3">/</span>
          <h1 className="text-[15px] font-medium">{item ? item.name : `Export #${dataExportId}`}</h1>
        </div>
        {item && (
          <div className="flex items-center gap-2">
            {item.file_url ? (
              <a
                href={item.file_url}
                className="inline-flex h-8 items-center rounded-lg border border-line bg-surface px-3 text-[13px] font-medium text-accent hover:bg-bg"
              >
                Download CSV
              </a>
            ) : (
              <Button variant="quiet" onClick={() => void handleDownload()}>
                Download CSV
              </Button>
            )}
            <Button onClick={() => void handleRerun()} disabled={busy}>
              {busy ? "Running..." : "Rerun export"}
            </Button>
          </div>
        )}
      </div>

      {notice && <p role="status" className="mb-4 text-[13px] text-teal-600">{notice}</p>}
      {error && <p role="alert" className="mb-4 text-[13px] text-red-600">{error}</p>}

      {loading ? (
        <p className="text-[13px] text-ink-3">Loading export details...</p>
      ) : item ? (
        <div className="max-w-3xl space-y-6">
          <dl className="grid grid-cols-1 gap-4 rounded-xl border border-line bg-surface p-5 sm:grid-cols-2">
            <div>
              <dt className="text-[12px] text-ink-3">Status</dt>
              <dd className="mt-1 flex items-center gap-2 text-[14px] font-medium capitalize">
                <span className={`size-2 rounded-full ${statusDot(item.status)}`} />
                {formatStatus(item.status)}
              </dd>
            </div>
            <div>
              <dt className="text-[12px] text-ink-3">Data type</dt>
              <dd className="mt-1 text-[14px] font-medium capitalize">{item.data_type}</dd>
            </div>
            <div>
              <dt className="text-[12px] text-ink-3">Processed records</dt>
              <dd className="mt-1 text-[14px] font-medium">
                {item.processed_records}
                {item.total_records !== null ? ` / ${item.total_records}` : ""}
              </dd>
            </div>
            <div>
              <dt className="text-[12px] text-ink-3">Initiated by</dt>
              <dd className="mt-1 text-[14px]">{item.initiated_by?.name ?? "System"}</dd>
            </div>
            <div>
              <dt className="text-[12px] text-ink-3">Created</dt>
              <dd className="mt-1 text-[13px] text-ink-2">{formatTs(item.created_at)}</dd>
            </div>
            <div>
              <dt className="text-[12px] text-ink-3">Completed</dt>
              <dd className="mt-1 text-[13px] text-ink-2">{formatTs(item.completed_at)}</dd>
            </div>
          </dl>

          {item.error_message && (
            <div className="rounded-xl border border-red-300 bg-red-50 p-4 text-[13px] text-red-700">
              <div className="font-medium">Export Error</div>
              <p className="mt-1">{item.error_message}</p>
            </div>
          )}

          {Object.keys(item.export_options ?? {}).length > 0 && (
            <section className="rounded-xl border border-line bg-surface p-5">
              <h2 className="mb-2 text-[13px] font-medium text-ink-2">Export Options</h2>
              <pre className="overflow-x-auto text-[12px] text-ink-2">
                {JSON.stringify(item.export_options, null, 2)}
              </pre>
            </section>
          )}
        </div>
      ) : null}
    </div>
  );
}
