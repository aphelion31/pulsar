import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Bot = { id: number; name: string; description: string | null; outgoing_url: string | null; system_bot: boolean };
type Form = { name: string; description: string; outgoing_url: string };
const empty: Form = { name: "", description: "", outgoing_url: "" };

export default function AgentBotsSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/agent_bots`;
  const [bots, setBots] = useState<Bot[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Bot | null>(null);
  const [mode, setMode] = useState<"add" | "edit" | "delete" | null>(null);
  const [form, setForm] = useState<Form>(empty);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function load() { setBots(await api<Bot[]>(base)); }
  useEffect(() => { setLoading(true); load().catch(e => setError(e.message)).finally(() => setLoading(false)); }, [base]);
  function open(bot?: Bot) {
    setSelected(bot ?? null);
    setForm(bot ? { name: bot.name, description: bot.description ?? "", outgoing_url: bot.outgoing_url ?? "" } : empty);
    setError(""); setMode(bot ? "edit" : "add");
  }
  async function save(e: React.FormEvent) {
    e.preventDefault(); setBusy(true); setError("");
    try { await api(selected ? `${base}/${selected.id}` : base, { method: selected ? "PATCH" : "POST", json: { ...form, bot_type: "webhook" } }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("AGENT_BOTS.ADD.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  async function remove() {
    if (!selected) return;
    setBusy(true); setError("");
    try { await api(`${base}/${selected.id}`, { method: "DELETE" }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("AGENT_BOTS.DELETE.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  const filtered = bots.filter(bot => `${bot.name} ${bot.description ?? ""}`.toLowerCase().includes(search.toLowerCase()));
  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4"><div><h1 className="text-xl font-medium">{t("AGENT_BOTS.HEADER")}</h1><p className="mt-2 text-sm text-n-slate-11">{t("AGENT_BOTS.DESCRIPTION")}</p></div><Button sm label={t("AGENT_BOTS.ADD.TITLE")} onClick={() => open()} /></header>
    <div className="mb-4 flex justify-end"><input aria-label={t("AGENT_BOTS.SEARCH_PLACEHOLDER")} placeholder={t("AGENT_BOTS.SEARCH_PLACEHOLDER")} value={search} onChange={e => setSearch(e.target.value)} className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm" /></div>
    {error && !mode && <p role="alert">{error}</p>}
    {loading ? <p>{t("AGENT_BOTS.LIST.LOADING")}</p> : !bots.length ? <p>{t("AGENT_BOTS.LIST.404")}</p> : <table className="w-full text-left text-sm"><thead><tr className="border-b border-n-weak text-n-slate-11">{["DETAILS", "URL", "ACTIONS"].map(key => <th key={key} className="p-3 font-medium">{t(`AGENT_BOTS.LIST.TABLE_HEADER.${key}`)}</th>)}</tr></thead><tbody>{filtered.map(bot => <tr key={bot.id} className="border-b border-n-weak"><td className="p-3">{bot.name}{bot.system_bot && ` (${t("AGENT_BOTS.GLOBAL_BOT_BADGE")})`}<p className="text-n-slate-11">{bot.description}</p></td><td className="p-3">{bot.outgoing_url}</td><td className="p-3"><div className="flex justify-end gap-3">{!bot.system_bot && <><Button sm slate ghost icon="pencil" aria-label={t("AGENT_BOTS.EDIT.BUTTON_TEXT")} onClick={() => open(bot)} /><Button sm slate ghost icon="trash-2" aria-label={t("AGENT_BOTS.DELETE.BUTTON_TEXT")} onClick={() => { setSelected(bot); setError(""); setMode("delete"); }} /></>}</div></td></tr>)}</tbody></table>}
    {!loading && bots.length > 0 && filtered.length === 0 && <p>{t("AGENT_BOTS.NO_RESULTS")}</p>}
    {mode && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"><div role="dialog" aria-modal="true" aria-labelledby="bot-title" className="w-full max-w-lg rounded-xl bg-n-background p-6 shadow-xl"><h2 id="bot-title" className="text-lg font-medium">{t(mode === "add" ? "AGENT_BOTS.ADD.TITLE" : mode === "edit" ? "AGENT_BOTS.EDIT.TITLE" : "AGENT_BOTS.DELETE.CONFIRM.TITLE")}</h2>
      {mode === "delete" ? <><p className="my-4">{t("AGENT_BOTS.DELETE.CONFIRM.MESSAGE", { name: selected?.name ?? "" })}</p>{error && <p role="alert">{error}</p>}<div className="mt-6 flex justify-end gap-2"><Button slate faded label={t("AGENT_BOTS.DELETE.CONFIRM.NO")} onClick={() => setMode(null)} /><Button ruby disabled={busy} label={t("AGENT_BOTS.DELETE.CONFIRM.YES")} onClick={remove} /></div></> : <form onSubmit={save} className="mt-4 grid gap-4">{(["name", "description", "outgoing_url"] as const).map(key => { const label = key === "outgoing_url" ? "WEBHOOK_URL" : key.toUpperCase(); return <label key={key} className="grid gap-1 text-sm">{t(`AGENT_BOTS.FORM.${label}.LABEL`)}<input required={key !== "description"} type={key === "outgoing_url" ? "url" : "text"} pattern={key === "outgoing_url" ? "https?://.*" : undefined} placeholder={t(`AGENT_BOTS.FORM.${label}.PLACEHOLDER`)} value={form[key]} onChange={e => setForm({ ...form, [key]: e.target.value })} className="rounded-lg border border-n-weak bg-n-background px-3 py-2" /></label>; })}{error && <p role="alert">{error}</p>}<div className="flex justify-end gap-2"><Button type="button" slate faded label={t("AGENT_BOTS.FORM.CANCEL")} onClick={() => setMode(null)} /><Button type="submit" disabled={busy || !form.name.trim()} label={t(mode === "add" ? "AGENT_BOTS.FORM.CREATE" : "AGENT_BOTS.FORM.UPDATE")} /></div></form>}
    </div></div>}
  </section>;
}
