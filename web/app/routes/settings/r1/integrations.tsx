import { useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import { Button, ErrorText, Input, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../../shell";

// mirrors chatwoot/app/javascript/dashboard/routes/dashboard/settings/integrations/{Index,DashboardApps}
type App = { id: number; name?: string; description?: string; logo?: string; enabled?: boolean };
type DashboardApp = { id: number; title: string; content: { type: string; url: string }[] };
const msg = (e: unknown) => (e instanceof Error ? e.message : "Request failed");
const list = <T,>(r: T[] | { payload: T[] }) => (Array.isArray(r) ? r : r.payload ?? []);
const card = "rounded-lg border border-line p-4 text-sm";

export default function Integrations() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [apps, setApps] = useState<App[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  useEffect(() => {
    api<App[] | { payload: App[] }>(`${acct(accountId)}/integrations/apps`)
      .then(r => setApps(list(r)))
      .catch(e => setError(msg(e)))
      .finally(() => setLoading(false));
  }, [accountId]);
  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={t("INTEGRATION_SETTINGS.HEADER")} />
      <div className="flex-1 overflow-auto p-6">
        <p className="mb-6 max-w-2xl text-sm text-ink-3">{t("INTEGRATION_SETTINGS.DESCRIPTION")}</p>
        <ErrorText>{error}</ErrorText>
        {loading ? <p className="text-sm text-ink-3">{t("INTEGRATION_SETTINGS.LOADING")}</p> : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            <Link to={`/accounts/${accountId}/settings/integrations/dashboard_apps`} className={card}>
              <p className="font-medium">{t("INTEGRATION_SETTINGS.DASHBOARD_APPS.TITLE")}</p>
              <p className="mt-1 text-ink-3">{t("INTEGRATION_SETTINGS.DASHBOARD_APPS.DESCRIPTION")}</p>
            </Link>
            {apps.map(app => (
              <div key={app.id} className={card}>
                <p className="font-medium">{app.name}</p>
                {app.description && <p className="mt-1 text-ink-3">{app.description}</p>}
                <p className="mt-2 text-ink-2">{app.enabled ? t("INTEGRATION_APPS.STATUS.ENABLED") : t("INTEGRATION_APPS.STATUS.DISABLED")}</p>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

const emptyForm = { title: "", url: "" };

export function DashboardApps() {
  const { accountId, me } = useOutletContext<ShellCtx>();
  const admin = me.accounts.find((a: { id: number; role: string }) => a.id === accountId)?.role === "administrator";
  const base = `${acct(accountId)}/dashboard_apps`;
  const [items, setItems] = useState<DashboardApp[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [form, setForm] = useState(emptyForm);
  const [editing, setEditing] = useState<number | null>(null);
  const [open, setOpen] = useState(false);
  const [confirmId, setConfirmId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  async function load() {
    setItems(list(await api<DashboardApp[] | { payload: DashboardApp[] }>(base)));
  }
  useEffect(() => {
    setLoading(true);
    load().catch(e => setError(msg(e))).finally(() => setLoading(false));
  }, [base]);

  function openCreate() { setEditing(null); setForm(emptyForm); setError(""); setOpen(true); }
  function openEdit(app: DashboardApp) { setEditing(app.id); setForm({ title: app.title, url: app.content?.[0]?.url ?? "" }); setError(""); setOpen(true); }

  const D = "INTEGRATION_SETTINGS.DASHBOARD_APPS";

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true); setError("");
    try {
      const dashboard_app = { title: form.title, content: [{ type: "frame", url: form.url }] };
      await api(editing === null ? base : `${base}/${editing}`, { method: editing === null ? "POST" : "PATCH", json: { dashboard_app } });
      setOpen(false);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : t(editing === null ? `${D}.CREATE.API_ERROR` : `${D}.UPDATE.API_ERROR`));
    } finally { setBusy(false); }
  }

  async function remove(app: DashboardApp) {
    setBusy(true); setError("");
    try {
      await api(`${base}/${app.id}`, { method: "DELETE" });
      setConfirmId(null);
      await load();
    } catch { setError(t("INTEGRATION_SETTINGS.DASHBOARD_APPS.DELETE.API_ERROR")); } finally { setBusy(false); }
  }

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={t(`${D}.TITLE`)}>
        {admin && <Button onClick={openCreate}>{t(`${D}.HEADER_BTN_TXT`)}</Button>}
      </PageHeader>
      <div className="flex-1 overflow-auto p-6">
        <ErrorText>{error}</ErrorText>
        {open && admin && (
          <form onSubmit={save} className="mb-4 grid gap-3 rounded-lg border border-line bg-bg p-4">
            <h3 className="text-[13px] font-medium">{t(editing === null ? `${D}.CREATE.HEADER` : `${D}.UPDATE.HEADER`)}</h3>
            <Input label={t(`${D}.FORM.TITLE_LABEL`)} required placeholder={t(`${D}.FORM.TITLE_PLACEHOLDER`)} value={form.title} onChange={e => setForm({ ...form, title: e.target.value })} />
            <Input label={t(`${D}.FORM.URL_LABEL`)} type="url" required placeholder={t(`${D}.FORM.URL_PLACEHOLDER`)} value={form.url} onChange={e => setForm({ ...form, url: e.target.value })} />
            <div className="flex justify-end gap-2">
              <Button type="button" variant="quiet" onClick={() => setOpen(false)}>{t(editing === null ? `${D}.CREATE.FORM_CANCEL` : `${D}.UPDATE.FORM_CANCEL`)}</Button>
              <Button disabled={busy}>{t(editing === null ? `${D}.CREATE.FORM_SUBMIT` : `${D}.UPDATE.FORM_SUBMIT`)}</Button>
            </div>
          </form>
        )}
        {loading ? <p className="text-sm text-ink-3">{t(`${D}.LIST.LOADING`)}</p> : items.length === 0 ? (
          <p className="text-sm text-ink-3">{t(`${D}.LIST.404`)}</p>
        ) : (
          <div className="overflow-x-auto rounded-lg border border-line">
            <table className="w-full text-left text-[13px]">
              <thead className="bg-bg text-ink-3">
                <tr>
                  <th className="px-4 py-3 font-medium">{t(`${D}.LIST.TABLE_HEADER.NAME`)}</th>
                  <th className="px-4 py-3 font-medium">{t(`${D}.LIST.TABLE_HEADER.ENDPOINT`)}</th>
                  {admin && <th className="px-4 py-3 font-medium">{t(`${D}.LIST.TABLE_HEADER.ACTIONS`)}</th>}
                </tr>
              </thead>
              <tbody>
                {items.map(app => (
                  <tr key={app.id} className="border-t border-line">
                    <td className="px-4 py-3 text-ink-2">{app.title}</td>
                    <td className="px-4 py-3 text-ink-2">{app.content?.[0]?.url ?? "—"}</td>
                    {admin && (
                      <td className="whitespace-nowrap px-4 py-2">
                        {confirmId === app.id ? (
                          <span className="grid gap-2">
                            <span>{t(`${D}.DELETE.MESSAGE`, { appName: app.title })}</span>
                            <span className="flex gap-2">
                              <Button variant="quiet" onClick={() => setConfirmId(null)}>{t(`${D}.DELETE.CONFIRM_NO`)}</Button>
                              <Button variant="danger" disabled={busy} onClick={() => remove(app)}>{t(`${D}.DELETE.CONFIRM_YES`)}</Button>
                            </span>
                          </span>
                        ) : (
                          <>
                            <Button variant="quiet" aria-label={t(`${D}.LIST.EDIT_TOOLTIP`)} onClick={() => openEdit(app)}>Edit</Button>
                            <Button variant="danger" aria-label={t(`${D}.LIST.DELETE_TOOLTIP`)} onClick={() => setConfirmId(app.id)}>Delete</Button>
                          </>
                        )}
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  );
}
