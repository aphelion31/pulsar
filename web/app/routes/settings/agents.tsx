import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Agent = { id: number; name: string; email: string; role: string; availability_status?: string };
type Form = { name: string; email: string; role: string; availability: string };
const empty: Form = { name: "", email: "", role: "agent", availability: "online" };
const input = "rounded-lg border border-n-weak bg-n-background px-3 py-2";

export default function AgentsSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/agents`;
  const [items, setItems] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Agent | null>(null);
  const [mode, setMode] = useState<"add" | "edit" | "delete" | null>(null);
  const [form, setForm] = useState<Form>(empty);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function load() { setItems(await api<Agent[]>(base)); }
  useEffect(() => { setLoading(true); load().catch(e => setError(e.message)).finally(() => setLoading(false)); }, [base]);
  function open(item?: Agent) {
    setSelected(item ?? null); setError("");
    setForm(item ? { name: item.name, email: item.email, role: item.role, availability: item.availability_status || "online" } : empty);
    setMode(item ? "edit" : "add");
  }
  async function submit(e: React.FormEvent) {
    e.preventDefault(); setBusy(true); setError("");
    try {
      await api(mode === "add" ? base : `${base}/${selected!.id}`, { method: mode === "add" ? "POST" : "PATCH", json: mode === "add" ? { name: form.name, email: form.email, role: form.role } : { name: form.name, role: form.role, availability: form.availability } });
      await load(); setMode(null);
    } catch (e) { setError(e instanceof Error ? e.message : t("AGENT_MGMT.ADD.API.ERROR_MESSAGE")); } finally { setBusy(false); }
  }
  async function remove() {
    if (!selected) return;
    setBusy(true); setError("");
    try { await api(`${base}/${selected.id}`, { method: "DELETE" }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("AGENT_MGMT.DELETE.API.ERROR_MESSAGE")); } finally { setBusy(false); }
  }
  const filtered = items.filter(a => `${a.name} ${a.email}`.toLowerCase().includes(search.toLowerCase().trim()));
  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4"><div><h1 className="text-xl font-medium">{t("AGENT_MGMT.HEADER")}</h1><p className="mt-2 text-sm text-n-slate-11">{t("AGENT_MGMT.DESCRIPTION")}</p></div><Button sm label={t("AGENT_MGMT.HEADER_BTN_TXT")} onClick={() => open()} /></header>
    <div className="mb-4 flex justify-end"><input aria-label={t("AGENT_MGMT.SEARCH_PLACEHOLDER")} placeholder={t("AGENT_MGMT.SEARCH_PLACEHOLDER")} value={search} onChange={e => setSearch(e.target.value)} className={input} /></div>
    {error && !mode && <p role="alert" className="mb-4 text-n-ruby-11">{error}</p>}
    {loading ? <p>{t("AGENT_MGMT.LOADING")}</p> : items.length === 0 ? <p>{t("AGENT_MGMT.LIST.404")}</p> : <div className="overflow-x-auto"><table className="w-full text-left text-sm"><thead><tr className="border-b border-n-weak text-n-slate-11">{["NAME", "EMAIL", "STATUS", "ACTIONS"].map(k => <th key={k} className="p-3 font-medium">{t(`AGENT_MGMT.LIST.${k}`)}</th>)}</tr></thead><tbody>{filtered.map(a => <tr key={a.id} className="border-b border-n-weak"><td className="p-3">{a.name}</td><td className="p-3">{a.email}</td><td className="p-3">{a.availability_status}</td><td className="p-3"><div className="flex justify-end gap-3"><Button sm slate ghost icon="pencil" aria-label={t("AGENT_MGMT.EDIT.BUTTON_TEXT")} onClick={() => open(a)} /><Button sm slate ghost icon="trash-2" aria-label={t("AGENT_MGMT.DELETE.BUTTON_TEXT")} onClick={() => { setSelected(a); setError(""); setMode("delete"); }} /></div></td></tr>)}</tbody></table></div>}
    {mode && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onMouseDown={e => { if (e.target === e.currentTarget && !busy) setMode(null); }}><div role="dialog" aria-modal="true" aria-labelledby="agents-dialog-title" className="w-full max-w-lg rounded-xl bg-n-background p-6 shadow-xl"><h2 id="agents-dialog-title" className="text-lg font-medium">{t(mode === "delete" ? "AGENT_MGMT.DELETE.CONFIRM.TITLE" : mode === "add" ? "AGENT_MGMT.ADD.TITLE" : "AGENT_MGMT.EDIT.TITLE")}</h2>
      {mode === "delete" ? <><p className="my-4">{t("AGENT_MGMT.DELETE.CONFIRM.MESSAGE")}{selected?.name}?</p>{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="mt-6 flex justify-end gap-2"><Button slate faded label={t("AGENT_MGMT.DELETE.CONFIRM.NO")} onClick={() => setMode(null)} /><Button ruby label={t("AGENT_MGMT.DELETE.CONFIRM.YES")} disabled={busy} onClick={remove} /></div></> : <form onSubmit={submit} className="mt-4 grid gap-4">{mode === "add" && <p className="text-sm text-n-slate-11">{t("AGENT_MGMT.ADD.DESC")}</p>}<label className="grid gap-1 text-sm">{t(`AGENT_MGMT.${mode === "add" ? "ADD" : "EDIT"}.FORM.NAME.LABEL`)}<input autoFocus required value={form.name} onChange={e => setForm({ ...form, name: e.target.value })} className={input} /></label><label className="grid gap-1 text-sm">{t("AGENT_MGMT.ADD.FORM.AGENT_TYPE.LABEL")}<select value={form.role} onChange={e => setForm({ ...form, role: e.target.value })} className={input}><option value="administrator">{t("AGENT_MGMT.AGENT_TYPES.ADMINISTRATOR")}</option><option value="agent">{t("AGENT_MGMT.AGENT_TYPES.AGENT")}</option></select></label><label className="grid gap-1 text-sm">{t("AGENT_MGMT.ADD.FORM.EMAIL.LABEL")}<input required type="email" disabled={mode === "edit"} value={form.email} onChange={e => setForm({ ...form, email: e.target.value })} className={input} /></label>{mode === "edit" && <label className="grid gap-1 text-sm">{t("AGENT_MGMT.EDIT.FORM.AGENT_AVAILABILITY.LABEL")}<select value={form.availability} onChange={e => setForm({ ...form, availability: e.target.value })} className={input}><option value="online">{t("PROFILE_SETTINGS.FORM.AVAILABILITY.STATUS.ONLINE")}</option><option value="busy">{t("PROFILE_SETTINGS.FORM.AVAILABILITY.STATUS.BUSY")}</option><option value="offline">{t("PROFILE_SETTINGS.FORM.AVAILABILITY.STATUS.OFFLINE")}</option></select></label>}{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="flex justify-end gap-2"><Button type="button" slate faded label={t("AGENT_MGMT.ADD.CANCEL_BUTTON_TEXT")} onClick={() => setMode(null)} /><Button type="submit" disabled={busy} label={t(mode === "add" ? "AGENT_MGMT.ADD.FORM.SUBMIT" : "AGENT_MGMT.EDIT.FORM.SUBMIT")} /></div></form>}
    </div></div>}
  </section>;
}
