import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useOutletContext } from "react-router";
import { Button } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";
import { ACTIVE, formatDate, formatStatus, importedCount, importTypeLabel, POLL_MS, statusDot } from "./data-import-utils";

export type DataImport = {
  id: number; name: string | null; data_type: string; status: string; total_records: number | null;
  processed_records: number | null; stats: Record<string, { imported?: number }>; created_at: string;
  import_types?: string[];
};

const PREFIX = "DATA_IMPORTS.";

export default function DataImportsSettings() {
  const { accountId, me } = useOutletContext<ShellCtx>();
  const navigate = useNavigate();
  const isAdmin = me.accounts?.some((a) => a.id === accountId && a.role === "administrator");
  const [items, setItems] = useState<DataImport[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [activeTab, setActiveTab] = useState<"import" | "export">("import");
  const [showNew, setShowNew] = useState(false);
  const [error, setError] = useState("");
  const timer = useRef<number | null>(null);
  const base = `${acct(accountId)}/data_imports`;
  const hasActive = items.some((i) => ACTIVE.includes(i.status));

  async function fetchImports() {
    const r = await api<{ payload: DataImport[] }>(base);
    setItems(r.payload);
  }

  async function refresh(showLoader = true) {
    if (showLoader) setLoading(true); else setRefreshing(true);
    try { await fetchImports(); setError(""); }
    catch (e) { setError(e instanceof Error ? e.message : ""); }
    finally { setLoading(false); setRefreshing(false); }
  }

  useEffect(() => { refresh(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [base]);

  // Poll every POLL_MS while an import is pending or processing. Pauses when the tab is hidden.
  useEffect(() => {
    if (!hasActive) return;
    const id = window.setInterval(() => { if (!document.hidden) fetchImports().catch(() => {}); }, POLL_MS);
    timer.current = id;
    return () => window.clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasActive, base]);

  if (!isAdmin) {
    return <div className="p-8 text-ink-3">You are not authorized to do this action</div>;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="border-b border-line px-6 py-4">
        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <h1 className="text-[15px] font-medium">{t(`${PREFIX}HEADER`)}</h1>
            {activeTab === "import" && items.length > 0 && (
              <span className="text-[13px] text-ink-3">{t(`${PREFIX}TABLE.COUNT`, { count: items.length })}</span>
            )}
          </div>
          {activeTab === "import" && (
            <div className="flex items-center gap-2">
              {hasActive && (
                <span className="hidden items-center gap-1.5 text-[13px] text-ink-3 sm:inline-flex">
                  <span className="size-2 animate-pulse rounded-full bg-teal-500" />
                  {t(`${PREFIX}MONITOR.LIVE`, { seconds: POLL_MS / 1000 })}
                </span>
              )}
              <Button variant="quiet" onClick={() => refresh(false)} aria-label={t(`${PREFIX}MONITOR.REFRESH`)} title={t(`${PREFIX}MONITOR.REFRESH`)} disabled={refreshing}>
                {refreshing ? t(`${PREFIX}MONITOR.REFRESHING`) : t(`${PREFIX}MONITOR.REFRESH`)}
              </Button>
              <Button onClick={() => setShowNew(true)}>{t(`${PREFIX}TABLE.NEW_IMPORT`)}</Button>
            </div>
          )}
        </div>
        <p className="mt-1 max-w-2xl text-[13px] text-ink-3">{t(`${PREFIX}DESCRIPTION`)}</p>
        <div role="tablist" className="mt-3 inline-flex gap-1 rounded-lg bg-bg p-1">
          {(["import", "export"] as const).map((key) => (
            <button key={key} role="tab" aria-selected={activeTab === key} onClick={() => setActiveTab(key)}
              className={`rounded-md px-3 py-1.5 text-[13px] transition-colors ${activeTab === key ? "bg-surface text-ink shadow-sm" : "text-ink-2 hover:text-ink"}`}>
              {t(`${PREFIX}TABS.${key.toUpperCase()}`)}
            </button>
          ))}
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto p-6">
        {error && <p role="alert" className="mb-4 text-[13px] text-red-600">{error}</p>}

        {activeTab === "export" ? (
          <Card icon="⇪" title={t(`${PREFIX}EXPORT.TITLE`)} body={t(`${PREFIX}EXPORT.DESCRIPTION`)}>
            <span className="rounded-md bg-bg px-2 py-1 text-[12px] text-ink-3">{t(`${PREFIX}EXPORT.COMING_SOON`)}</span>
          </Card>
        ) : loading ? (
          <p className="text-[13px] text-ink-3">{t(`${PREFIX}LOADING`)}</p>
        ) : items.length === 0 ? (
          <Card icon="▤" title={t(`${PREFIX}TABLE.EMPTY`)} body={t(`${PREFIX}TABLE.EMPTY_DESCRIPTION`)}>
            <Button onClick={() => setShowNew(true)}>{t(`${PREFIX}TABLE.NEW_IMPORT`)}</Button>
          </Card>
        ) : (
          <ul className="divide-y divide-line border-y border-line">
            {items.map((item) => (
              <li key={item.id}>
                <Link to={`/accounts/${accountId}/settings/data/${item.id}`} className="flex items-center justify-between gap-4 py-4 hover:bg-bg/60">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-[14px] font-medium">{item.name || t(`${PREFIX}TABLE.UNNAMED`)}</span>
                      <span className="flex shrink-0 items-center gap-1.5">
                        <span className={`size-2 rounded-full ${statusDot(item.status)} ${ACTIVE.includes(item.status) ? "animate-pulse" : ""}`} />
                        <span className="whitespace-nowrap text-[13px] capitalize text-ink-3">{formatStatus(item.status)}</span>
                      </span>
                    </div>
                    <div className="mt-1 flex flex-wrap items-center gap-2 text-[13px] text-ink-3">
                      <span>{importTypeLabel(item)}</span><span className="h-3 w-px bg-line" />
                      <span className="tabular-nums">{t(`${PREFIX}TABLE.IMPORTED_COUNT`, { count: importedCount(item) })}</span><span className="h-3 w-px bg-line" />
                      <span>{formatDate(item.created_at)}</span>
                    </div>
                  </div>
                  <span className="shrink-0 text-[13px] text-accent" aria-hidden>{t(`${PREFIX}TABLE.VIEW`)}</span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </div>

      {showNew && <NewImportPanel base={base} onClose={() => setShowNew(false)} onCreated={(id) => { setShowNew(false); navigate(`/accounts/${accountId}/settings/data/${id}`); }} />}
    </div>
  );
}

function Card({ icon, title, body, children }: { icon: string; title: string; body: string; children?: React.ReactNode }) {
  return (
    <div className="flex min-h-80 flex-col items-center justify-center gap-4 rounded-xl border border-line bg-surface px-6 py-16 text-center">
      <span className="grid size-12 place-items-center rounded-full bg-bg text-ink-3">{icon}</span>
      <div className="flex flex-col gap-1">
        <h3 className="text-[15px] font-medium">{title}</h3>
        <p className="max-w-sm text-[13px] text-ink-3">{body}</p>
      </div>
      {children}
    </div>
  );
}

// Contacts CSV only. Chatwoot's drawer also covers Intercom and Freshdesk, which Pulsar does not support.
function NewImportPanel({ base, onClose, onCreated }: { base: string; onClose: () => void; onCreated: (id: number) => void }) {
  const [file, setFile] = useState<File | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function create(e: React.FormEvent) {
    e.preventDefault();
    if (!file) return;
    setBusy(true); setError("");
    try {
      const body = new FormData();
      body.append("import_file", file);
      body.append("data_type", "contacts");
      if (name.trim()) body.append("name", name.trim());
      const created = await api<{ id: number }>(base, { method: "POST", body });
      onCreated(created.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : t(`${PREFIX}ALERTS.IMPORT_FAILED`));
    } finally { setBusy(false); }
  }
  return (
    <div role="dialog" aria-modal="true" aria-label={t(`${PREFIX}DRAWER.TITLE`)} className="fixed inset-0 z-50 grid place-items-center bg-black/30 p-4">
      <form onSubmit={create} className="w-full max-w-md space-y-4 rounded-xl bg-surface p-6 shadow-xl">
        <h2 className="text-[15px] font-medium">{t(`${PREFIX}DRAWER.TITLE`)}</h2>
        <label className="block text-[13px]">
          <span className="mb-1 block text-ink-2">{t(`${PREFIX}DRAWER.NAME`)}</span>
          <input className="w-full rounded-lg border border-line bg-bg px-3 py-2" value={name} placeholder={t(`${PREFIX}DRAWER.NAME_PLACEHOLDER`)} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="block text-[13px]">
          <span className="mb-1 block text-ink-2">{t(`${PREFIX}TYPES.CONTACTS`)}</span>
          <input type="file" accept=".csv,text/csv" onChange={(e) => setFile(e.target.files?.[0] ?? null)} className="w-full text-[13px]" />
        </label>
        {error && <p role="alert" className="text-[13px] text-red-600">{error}</p>}
        <div className="flex justify-end gap-2">
          <Button variant="quiet" type="button" onClick={onClose}>{t(`${PREFIX}DRAWER.CANCEL`)}</Button>
          <Button type="submit" disabled={!file || busy}>{t(`${PREFIX}DRAWER.IMPORT`)}</Button>
        </div>
      </form>
    </div>
  );
}
