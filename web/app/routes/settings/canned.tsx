import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Response = { id: number; short_code: string; content: string };
export default function CannedSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/canned_responses`;
  const [items, setItems] = useState<Response[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [descending, setDescending] = useState(false);
  const [mode, setMode] = useState<"add" | "edit" | "delete" | null>(null);
  const [selected, setSelected] = useState<Response | null>(null);
  const [code, setCode] = useState("");
  const [content, setContent] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function load() { const data = await api<Response[] | { payload: Response[] }>(base); setItems(Array.isArray(data) ? data : data.payload); }
  useEffect(() => { setLoading(true); load().catch(e => setError(e.message)).finally(() => setLoading(false)); }, [base]);
  function open(item?: Response) { setSelected(item ?? null); setCode(item?.short_code ?? ""); setContent(item?.content ?? ""); setError(""); setMode(item ? "edit" : "add"); }
  async function save(e: React.FormEvent) {
    e.preventDefault(); setBusy(true); setError("");
    try { await api(mode === "add" ? base : `${base}/${selected!.id}`, { method: mode === "add" ? "POST" : "PATCH", json: { short_code: code, content } }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("CANNED_MGMT.ADD.API.ERROR_MESSAGE")); } finally { setBusy(false); }
  }
  async function remove() {
    if (!selected) return;
    setBusy(true); setError("");
    try { await api(`${base}/${selected.id}`, { method: "DELETE" }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("CANNED_MGMT.DELETE.API.ERROR_MESSAGE")); } finally { setBusy(false); }
  }
  const filtered = items.filter(item => `${item.short_code} ${item.content}`.toLowerCase().includes(search.trim().toLowerCase())).sort((a, b) => (descending ? -1 : 1) * a.short_code.localeCompare(b.short_code));
  const section = mode === "edit" ? "EDIT" : "ADD";
  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    <header className="mb-6 flex flex-wrap justify-between gap-4"><div><h1 className="text-xl font-medium">{t("CANNED_MGMT.HEADER")}</h1><p className="mt-2 text-sm text-n-slate-11">{t("CANNED_MGMT.DESCRIPTION")}</p><a className="text-sm text-n-blue-11" href="https://www.chatwoot.com/hc/user-guide/articles/1677693051-how-to-use-canned-responses" target="_blank" rel="noreferrer">{t("CANNED_MGMT.LEARN_MORE")}</a></div><Button sm label={t("CANNED_MGMT.HEADER_BTN_TXT")} onClick={() => open()} /></header>
    <div className="mb-4 flex justify-end"><input aria-label={t("CANNED_MGMT.SEARCH_PLACEHOLDER")} placeholder={t("CANNED_MGMT.SEARCH_PLACEHOLDER")} value={search} onChange={e => setSearch(e.target.value)} className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm" /></div>
    {error && !mode && <p role="alert" className="text-n-ruby-11">{error}</p>}
    {loading ? <p>{t("CANNED_MGMT.LOADING")}</p> : !items.length ? <p>{t("CANNED_MGMT.LIST.404")}</p> : <table className="w-full text-left text-sm"><thead><tr className="border-b border-n-weak text-n-slate-11"><th className="p-3 font-medium"><button onClick={() => setDescending(!descending)}>{t("CANNED_MGMT.LIST.TABLE_HEADER.SHORT_CODE")} {descending ? "↓" : "↑"}</button></th><th className="p-3 text-right font-medium">{t("CANNED_MGMT.LIST.TABLE_HEADER.ACTIONS")}</th></tr></thead><tbody>{filtered.map(item => <tr className="border-b border-n-weak" key={item.id}><td className="p-3"><strong className="font-medium">{item.short_code}</strong><p className="mt-2 line-clamp-5 text-n-slate-11">{item.content}</p></td><td className="p-3"><div className="flex justify-end gap-3"><Button sm slate ghost icon="pencil" aria-label={t("CANNED_MGMT.EDIT.BUTTON_TEXT")} onClick={() => open(item)} /><Button sm slate ghost icon="trash-2" aria-label={t("CANNED_MGMT.DELETE.BUTTON_TEXT")} onClick={() => { setSelected(item); setError(""); setMode("delete"); }} /></div></td></tr>)}</tbody></table>}
    {!!items.length && !filtered.length && <p>{t("CANNED_MGMT.NO_RESULTS")}</p>}
    {mode && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"><div role="dialog" aria-modal="true" aria-labelledby="canned-title" className="w-full max-w-lg rounded-xl bg-n-background p-6 shadow-xl"><h2 id="canned-title" className="text-lg font-medium">{mode === "delete" ? t("CANNED_MGMT.DELETE.CONFIRM.TITLE") : mode === "edit" ? `${t("CANNED_MGMT.EDIT.TITLE")} - ${selected?.short_code}` : t("CANNED_MGMT.ADD.TITLE")}</h2>
      {mode === "delete" ? <><p className="my-4">{t("CANNED_MGMT.DELETE.CONFIRM.MESSAGE")} {selected?.short_code}?</p>{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="flex justify-end gap-2"><Button slate faded label={`${t("CANNED_MGMT.DELETE.CONFIRM.NO")} ${selected?.short_code}`} onClick={() => setMode(null)} /><Button ruby disabled={busy} label={`${t("CANNED_MGMT.DELETE.CONFIRM.YES")} ${selected?.short_code}`} onClick={remove} /></div></> : <form onSubmit={save} className="mt-4 grid gap-4">{mode === "add" && <p className="text-sm text-n-slate-11">{t("CANNED_MGMT.ADD.DESC")}</p>}<label className="grid gap-1 text-sm">{t(`CANNED_MGMT.${section}.FORM.SHORT_CODE.LABEL`)}<input autoFocus minLength={2} required value={code} onChange={e => setCode(e.target.value)} placeholder={t(`CANNED_MGMT.${section}.FORM.SHORT_CODE.PLACEHOLDER`)} className="rounded-lg border border-n-weak bg-n-background px-3 py-2" /></label><label className="grid gap-1 text-sm">{t(`CANNED_MGMT.${section}.FORM.CONTENT.LABEL`)}<textarea required rows={8} value={content} onChange={e => setContent(e.target.value)} placeholder={t(`CANNED_MGMT.${section}.FORM.CONTENT.PLACEHOLDER`)} className="rounded-lg border border-n-weak bg-n-background px-3 py-2" /></label>{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="flex justify-end gap-2"><Button type="button" slate faded label={t(`CANNED_MGMT.${section}.CANCEL_BUTTON_TEXT`)} onClick={() => setMode(null)} /><Button type="submit" disabled={busy || code.trim().length < 2 || !content.trim()} label={t(`CANNED_MGMT.${section}.FORM.SUBMIT`)} /></div></form>}
    </div></div>}
  </section>;
}
