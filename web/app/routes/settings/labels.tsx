import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Label = { id: number; title: string; description: string | null; color: string; show_on_sidebar: boolean };
type Form = { title: string; description: string; color: string; show_on_sidebar: boolean };
const empty: Form = { title: "", description: "", color: "#1f93ff", show_on_sidebar: true };

export default function LabelsSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/labels`;
  const [items, setItems] = useState<Label[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Label | null>(null);
  const [mode, setMode] = useState<"add" | "edit" | "delete" | null>(null);
  const [form, setForm] = useState<Form>(empty);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function load() {
    const data = await api<Label[] | { payload: Label[] }>(base);
    setItems(Array.isArray(data) ? data : data.payload);
  }
  useEffect(() => { setLoading(true); load().catch(e => setError(e.message)).finally(() => setLoading(false)); }, [base]);
  function open(item?: Label) {
    setError(""); setSelected(item ?? null);
    setForm(item ? { title: item.title, description: item.description ?? "", color: item.color, show_on_sidebar: item.show_on_sidebar } : empty);
    setMode(item ? "edit" : "add");
  }
  async function submit(e: React.FormEvent) {
    e.preventDefault(); setBusy(true); setError("");
    try {
      await api(mode === "add" ? base : `${base}/${selected!.id}`, { method: mode === "add" ? "POST" : "PATCH", json: { ...form, title: form.title.toLowerCase() } });
      await load(); setMode(null);
    } catch (e) { setError(e instanceof Error ? e.message : t("LABEL_MGMT.ADD.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  async function remove() {
    if (!selected) return;
    setBusy(true); setError("");
    try { await api(`${base}/${selected.id}`, { method: "DELETE" }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("LABEL_MGMT.DELETE.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  const filtered = items.filter(item => `${item.title} ${item.description ?? ""}`.toLowerCase().includes(search.trim().toLowerCase()));
  const valid = /^[a-zA-Z0-9_-]{2,}$/.test(form.title);
  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
      <div><h1 className="text-xl font-medium">{t("LABEL_MGMT.HEADER")}</h1><p className="mt-2 text-sm text-n-slate-11">{t("LABEL_MGMT.DESCRIPTION")}</p><a className="text-sm text-n-blue-11" href="https://www.chatwoot.com/hc/user-guide/articles/1677693021-how-to-use-labels" target="_blank" rel="noreferrer">{t("LABEL_MGMT.LEARN_MORE")}</a></div>
      <Button sm label={t("LABEL_MGMT.HEADER_BTN_TXT")} onClick={() => open()} />
    </header>
    <div className="mb-4 flex items-center justify-between gap-3"><span className="text-sm text-n-slate-11">{items.length > 0 && (items.length === 1 ? "1 label" : `${items.length} labels`)}</span><input aria-label={t("LABEL_MGMT.SEARCH_PLACEHOLDER")} placeholder={t("LABEL_MGMT.SEARCH_PLACEHOLDER")} value={search} onChange={e => setSearch(e.target.value)} className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm" /></div>
    {error && !mode && <p role="alert" className="mb-4 text-n-ruby-11">{error}</p>}
    {loading ? <p>{t("LABEL_MGMT.LOADING")}</p> : items.length === 0 ? <p className="text-sm text-n-slate-11">{t("LABEL_MGMT.LIST.404")}</p> : <div className="overflow-x-auto"><table className="w-full text-left text-sm"><thead><tr className="border-b border-n-weak text-n-slate-11">{["NAME", "DESCRIPTION", "COLOR", "ACTION"].map(k => <th className="p-3 font-medium" key={k}>{t(`LABEL_MGMT.LIST.TABLE_HEADER.${k}`)}</th>)}</tr></thead><tbody>{filtered.map(item => <tr key={item.id} className="border-b border-n-weak"><td className="p-3">{item.title}</td><td className="w-full p-3 text-n-slate-11">{item.description}</td><td className="whitespace-nowrap p-3"><span className="mr-2 inline-block h-4 w-4 rounded border border-n-weak align-middle" style={{ backgroundColor: item.color }} />{item.color}</td><td className="p-3"><div className="flex justify-end gap-3"><Button sm slate ghost aria-label={t("LABEL_MGMT.FORM.EDIT")} icon="pencil" onClick={() => open(item)} /><Button sm slate ghost aria-label={t("LABEL_MGMT.FORM.DELETE")} icon="trash-2" onClick={() => { setError(""); setSelected(item); setMode("delete"); }} /></div></td></tr>)}</tbody></table>{filtered.length === 0 && <p className="p-4">{t("LABEL_MGMT.NO_RESULTS")}</p>}</div>}
    {mode && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onMouseDown={e => { if (e.target === e.currentTarget && !busy) setMode(null); }}><div role="dialog" aria-modal="true" aria-labelledby="labels-dialog-title" className="w-full max-w-lg rounded-xl bg-n-background p-6 shadow-xl"><h2 id="labels-dialog-title" className="text-lg font-medium">{mode === "add" ? t("LABEL_MGMT.ADD.TITLE") : mode === "edit" ? `${t("LABEL_MGMT.EDIT.TITLE")} - ${selected?.title}` : t("LABEL_MGMT.DELETE.CONFIRM.TITLE")}</h2>
      {mode === "delete" ? <><p className="my-4">{t("LABEL_MGMT.DELETE.CONFIRM.MESSAGE")} {selected?.title}?</p>{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="mt-6 flex justify-end gap-2"><Button slate faded label={t("LABEL_MGMT.DELETE.CONFIRM.NO")} onClick={() => setMode(null)} /><Button ruby label={t("LABEL_MGMT.DELETE.CONFIRM.YES")} disabled={busy} onClick={remove} /></div></> : <form onSubmit={submit} className="mt-4 grid gap-4">{mode === "add" && <p className="text-sm text-n-slate-11">{t("LABEL_MGMT.ADD.DESC")}</p>}<label className="grid gap-1 text-sm">{t("LABEL_MGMT.FORM.NAME.LABEL")}<input autoFocus required minLength={2} pattern="[A-Za-z0-9_-]+" value={form.title} onChange={e => setForm({ ...form, title: e.target.value })} placeholder={t("LABEL_MGMT.FORM.NAME.PLACEHOLDER")} className="rounded-lg border border-n-weak bg-n-background px-3 py-2 lowercase" /></label><label className="grid gap-1 text-sm">{t("LABEL_MGMT.FORM.DESCRIPTION.LABEL")}<input value={form.description} onChange={e => setForm({ ...form, description: e.target.value })} placeholder={t("LABEL_MGMT.FORM.DESCRIPTION.PLACEHOLDER")} className="rounded-lg border border-n-weak bg-n-background px-3 py-2" /></label><label className="grid gap-1 text-sm">{t("LABEL_MGMT.FORM.COLOR.LABEL")}<input type="color" value={form.color} onChange={e => setForm({ ...form, color: e.target.value })} /></label><label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={form.show_on_sidebar} onChange={e => setForm({ ...form, show_on_sidebar: e.target.checked })} />{t("LABEL_MGMT.FORM.SHOW_ON_SIDEBAR.LABEL")}</label>{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="flex justify-end gap-2"><Button type="button" slate faded label={t("LABEL_MGMT.FORM.CANCEL")} onClick={() => setMode(null)} /><Button type="submit" disabled={!valid || busy} label={t(mode === "add" ? "LABEL_MGMT.FORM.CREATE" : "LABEL_MGMT.FORM.EDIT")} /></div></form>}
    </div></div>}
  </section>;
}
