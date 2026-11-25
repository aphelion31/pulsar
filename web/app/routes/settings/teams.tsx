import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Team = { id: number; name: string; description: string | null; allow_auto_assign: boolean };
type Agent = { id: number; name: string; email: string };
const input = "rounded-lg border border-n-weak bg-n-background px-3 py-2";
export default function TeamsSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/teams`;
  const [teams, setTeams] = useState<Team[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [team, setTeam] = useState<Team | null>(null);
  const [mode, setMode] = useState<"add" | "edit" | "agents" | "delete" | null>(null);
  const [form, setForm] = useState({ name: "", description: "", allow_auto_assign: true });
  const [selected, setSelected] = useState<number[]>([]);
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function load() { setTeams(await api<Team[]>(base)); }
  useEffect(() => { setLoading(true); load().catch(e => setError(e.message)).finally(() => setLoading(false)); }, [base]);
  function open(item?: Team) { setError(""); setTeam(item ?? null); setForm(item ? { name: item.name, description: item.description ?? "", allow_auto_assign: item.allow_auto_assign } : { name: "", description: "", allow_auto_assign: true }); setMode(item ? "edit" : "add"); }
  async function save(e: React.FormEvent) {
    e.preventDefault(); setBusy(true); setError("");
    try {
      const saved = await api<Team>(mode === "add" ? base : `${base}/${team!.id}`, { method: mode === "add" ? "POST" : "PATCH", json: form });
      setTeam(saved); await load();
      const [all, members] = await Promise.all([api<Agent[]>(`${acct(accountId)}/agents`), api<Agent[]>(`${base}/${saved.id}/team_members`)]);
      setAgents(all); setSelected(members.map(a => a.id)); setMode("agents");
    } catch (e) { setError(e instanceof Error ? e.message : t("TEAMS_SETTINGS.TEAM_FORM.ERROR_MESSAGE")); } finally { setBusy(false); }
  }
  async function saveAgents() {
    if (!team) return;
    setBusy(true); setError("");
    try { await api(`${base}/${team.id}/team_members`, { method: "PATCH", json: { user_ids: selected } }); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("TEAMS_SETTINGS.TEAM_FORM.ERROR_MESSAGE")); } finally { setBusy(false); }
  }
  async function remove() {
    if (!team) return;
    setBusy(true); setError("");
    try { await api(`${base}/${team.id}`, { method: "DELETE" }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("TEAMS_SETTINGS.DELETE.API.ERROR_MESSAGE")); } finally { setBusy(false); }
  }
  return <section className="flex-1 overflow-auto p-8 text-n-slate-12"><header className="mb-6 flex flex-wrap items-start justify-between gap-4"><div><h1 className="text-xl font-medium">{t("TEAMS_SETTINGS.HEADER")}</h1><p className="mt-2 text-sm text-n-slate-11">{t("TEAMS_SETTINGS.DESCRIPTION")}</p></div><Button sm label={t("TEAMS_SETTINGS.NEW_TEAM")} onClick={() => open()} /></header>
    <div className="mb-4 flex justify-end"><input aria-label={t("TEAMS_SETTINGS.SEARCH_PLACEHOLDER")} placeholder={t("TEAMS_SETTINGS.SEARCH_PLACEHOLDER")} value={search} onChange={e => setSearch(e.target.value)} className={input} /></div>{error && !mode && <p role="alert" className="text-n-ruby-11">{error}</p>}
    {loading ? <p>{t("TEAMS_SETTINGS.LOADING")}</p> : teams.length === 0 ? <p>{t("TEAMS_SETTINGS.LIST.404")}</p> : <div>{teams.filter(x => `${x.name} ${x.description ?? ""}`.toLowerCase().includes(search.toLowerCase().trim())).map(x => <div key={x.id} className="flex items-center justify-between border-b border-n-weak p-3"><div><div>{x.name}</div><div className="text-sm text-n-slate-11">{x.description}</div></div><div className="flex gap-2"><Button sm slate ghost icon="pencil" aria-label={t("TEAMS_SETTINGS.LIST.EDIT_TEAM")} onClick={() => open(x)} /><Button sm slate ghost icon="trash-2" aria-label={t("TEAMS_SETTINGS.DELETE.BUTTON_TEXT")} onClick={() => { setTeam(x); setConfirm(""); setError(""); setMode("delete"); }} /></div></div>)}</div>}
    {mode && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"><div role="dialog" aria-modal="true" aria-labelledby="team-dialog" className="w-full max-w-lg rounded-xl bg-n-background p-6 shadow-xl"><h2 id="team-dialog" className="text-lg font-medium">{mode === "agents" ? t("TEAMS_SETTINGS.CREATE_FLOW.AGENTS.TITLE", { teamName: team?.name ?? "" }) : t(mode === "delete" ? "TEAMS_SETTINGS.DELETE.CONFIRM.TITLE" : mode === "add" ? "TEAMS_SETTINGS.CREATE_FLOW.CREATE.TITLE" : "TEAMS_SETTINGS.EDIT_FLOW.CREATE.TITLE")}</h2>
      {mode === "delete" ? <div className="mt-4 grid gap-4"><p>{t("TEAMS_SETTINGS.DELETE.CONFIRM.MESSAGE")}</p><input aria-label={t("TEAMS_SETTINGS.DELETE.CONFIRM.PLACE_HOLDER", { teamName: team?.name ?? "" })} placeholder={t("TEAMS_SETTINGS.DELETE.CONFIRM.PLACE_HOLDER", { teamName: team?.name ?? "" })} value={confirm} onChange={e => setConfirm(e.target.value)} className={input} /><div className="flex justify-end gap-2"><Button slate faded label={t("TEAMS_SETTINGS.DELETE.CONFIRM.NO")} onClick={() => setMode(null)} /><Button ruby disabled={busy || confirm !== team?.name} label={t("TEAMS_SETTINGS.DELETE.CONFIRM.YES")} onClick={remove} /></div></div> : mode === "agents" ? <div className="mt-4 grid gap-4"><p className="text-sm text-n-slate-11">{t("TEAMS_SETTINGS.CREATE_FLOW.AGENTS.DESC")}</p><div className="max-h-72 overflow-auto">{agents.map(a => <label key={a.id} className="flex items-center gap-3 border-b border-n-weak p-2"><input type="checkbox" checked={selected.includes(a.id)} onChange={e => setSelected(e.target.checked ? [...selected, a.id] : selected.filter(id => id !== a.id))} /><span>{a.name} <span className="text-n-slate-11">{a.email}</span></span></label>)}</div><div className="flex justify-end gap-2"><Button slate faded label={t("TEAMS_SETTINGS.DELETE.CONFIRM.NO")} onClick={() => setMode(null)} /><Button disabled={busy} label={t("TEAMS_SETTINGS.AGENTS.BUTTON_TEXT")} onClick={saveAgents} /></div></div> : <form onSubmit={save} className="mt-4 grid gap-4"><p className="text-sm text-n-slate-11">{t(mode === "add" ? "TEAMS_SETTINGS.CREATE_FLOW.CREATE.DESC" : "TEAMS_SETTINGS.EDIT_FLOW.CREATE.DESC")}</p><label className="grid gap-1 text-sm">{t("TEAMS_SETTINGS.FORM.NAME.LABEL")}<input required autoFocus value={form.name} placeholder={t("TEAMS_SETTINGS.FORM.NAME.PLACEHOLDER")} onChange={e => setForm({ ...form, name: e.target.value })} className={input} /></label><label className="grid gap-1 text-sm">{t("TEAMS_SETTINGS.FORM.DESCRIPTION.LABEL")}<textarea value={form.description} placeholder={t("TEAMS_SETTINGS.FORM.DESCRIPTION.PLACEHOLDER")} onChange={e => setForm({ ...form, description: e.target.value })} className={input} /></label><label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={form.allow_auto_assign} onChange={e => setForm({ ...form, allow_auto_assign: e.target.checked })} />{t("TEAMS_SETTINGS.FORM.AUTO_ASSIGN.LABEL")}</label><div className="flex justify-end gap-2"><Button type="button" slate faded label={t("TEAMS_SETTINGS.DELETE.CONFIRM.NO")} onClick={() => setMode(null)} /><Button type="submit" disabled={busy} label={t(mode === "add" ? "TEAMS_SETTINGS.FORM.CREATE" : "TEAMS_SETTINGS.FORM.UPDATE")} /></div></form>}{error && <p role="alert" className="mt-4 text-n-ruby-11">{error}</p>}</div></div>}
  </section>;
}
