import { useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useOutletContext, useParams } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Team = { id: number; name: string; description: string | null; allow_auto_assign: boolean };
type Agent = { id: number; name: string; email: string };
const input = "rounded-lg border border-n-weak bg-n-background px-3 py-2";

export default function TeamEditor() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { teamId } = useParams();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const editing = pathname.includes("/edit");
  const step = pathname.endsWith("/finish") ? 2 : pathname.endsWith("/agents") ? 1 : 0;
  const base = `${acct(accountId)}/teams`;
  const root = `/accounts/${accountId}/settings/teams`;
  const [team, setTeam] = useState<Team | null>(null);
  const [form, setForm] = useState({ name: "", description: "", allow_auto_assign: true });
  const [agents, setAgents] = useState<Agent[]>([]);
  const [selected, setSelected] = useState<number[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    setError("");
    if (!teamId || step === 2) { setLoading(false); return; }
    setLoading(true);
    Promise.all([
      api<Team>(`${base}/${teamId}`),
      step === 1 ? api<Agent[]>(`${acct(accountId)}/agents`) : Promise.resolve([]),
      step === 1 && editing ? api<Agent[]>(`${base}/${teamId}/team_members`) : Promise.resolve([]),
    ]).then(([item, all, members]) => {
      if (!active) return;
      setTeam(item);
      setForm({ name: item.name, description: item.description ?? "", allow_auto_assign: item.allow_auto_assign });
      setAgents(all);
      setSelected(members.map(a => a.id));
    }).catch(e => { if (active) setError(e instanceof Error ? e.message : t("TEAMS_SETTINGS.TEAM_FORM.ERROR_MESSAGE")); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [base, accountId, teamId, step, editing]);

  async function saveDetails(e: React.FormEvent) {
    e.preventDefault();
    if (form.name.trim().length < 2 || busy) return;
    setBusy(true); setError("");
    try {
      const saved = await api<Team>(editing ? `${base}/${teamId}` : base, { method: editing ? "PATCH" : "POST", json: form });
      navigate(editing ? `${root}/${teamId}/edit/agents` : `${root}/new/${saved.id}/agents`, { replace: true });
    } catch (e) { setError(e instanceof Error ? e.message : t("TEAMS_SETTINGS.TEAM_FORM.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  async function saveAgents(e: React.FormEvent) {
    e.preventDefault();
    if (!selected.length || busy) return;
    setBusy(true); setError("");
    try {
      await api(`${base}/${teamId}/team_members`, { method: editing ? "PATCH" : "POST", json: { user_ids: selected } });
      navigate(editing ? `${root}/${teamId}/edit/finish` : `${root}/new/${teamId}/finish`, { replace: true });
    } catch (e) { setError(e instanceof Error ? e.message : t("TEAMS_SETTINGS.TEAM_FORM.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  const steps = editing ? ["EDIT_WIZARD_DETAILS", "EDIT_WIZARD_AGENTS", "EDIT_WIZARD_FINISH"] : ["WIZARD_CREATE", "WIZARD_ADD_AGENTS", "WIZARD_FINISH"];
  const flow = editing ? "EDIT_FLOW" : "CREATE_FLOW";
  return <section className="flex-1 overflow-auto p-6 text-n-slate-12"><div className="mx-auto grid min-h-[50dvh] max-w-7xl grid-cols-1 rounded-xl border border-n-weak lg:grid-cols-8 lg:divide-x lg:divide-n-weak">
    <ol className="hidden space-y-6 px-6 py-8 lg:col-span-2 lg:block">{steps.map((s, i) => <li key={s} aria-current={step === i ? "step" : undefined}><div className={step === i ? "font-semibold" : "font-medium text-n-slate-11"}>{i + 1}. {t(`TEAMS_SETTINGS.${flow}.${s}.TITLE`)}</div><p className="mt-1 text-sm text-n-slate-11">{t(`TEAMS_SETTINGS.${flow}.${s}.BODY`)}</p></li>)}</ol>
    <div className="p-8 lg:col-span-6">{step === 2 ? <div className="flex h-full flex-col items-center justify-center gap-4 text-center"><h1 className="text-xl font-medium">{t("TEAMS_SETTINGS.FINISH.TITLE")}</h1><p className="text-n-slate-11">{t("TEAMS_SETTINGS.FINISH.MESSAGE")}</p><Link to={`${root}/list`} className="rounded-lg bg-n-blue-9 px-4 py-2 text-white">{t("TEAMS_SETTINGS.FINISH.BUTTON_TEXT")}</Link></div> : <>
      <h1 className="text-xl font-medium">{step === 0 ? t(`TEAMS_SETTINGS.${flow}.CREATE.TITLE`) : t(`TEAMS_SETTINGS.${editing ? "EDIT_FLOW.AGENTS" : "ADD"}.TITLE`, { teamName: team?.name ?? "" })}</h1>
      <p className="mb-6 mt-2 text-sm text-n-slate-11">{t(`TEAMS_SETTINGS.${step === 0 ? `${flow}.CREATE` : editing ? "EDIT_FLOW.AGENTS" : "ADD"}.DESC`)}</p>
      {error && <p role="alert" className="mb-4 text-n-ruby-11">{error}</p>}
      {loading ? <p>{t("TEAMS_SETTINGS.LOADING")}</p> : step === 0 ? (!editing || team) && <form onSubmit={saveDetails} className="grid gap-4"><label className="grid gap-1 text-sm">{t("TEAMS_SETTINGS.FORM.NAME.LABEL")}<input required minLength={2} value={form.name} placeholder={t("TEAMS_SETTINGS.FORM.NAME.PLACEHOLDER")} onChange={e => setForm({ ...form, name: e.target.value })} className={input} /></label><label className="grid gap-1 text-sm">{t("TEAMS_SETTINGS.FORM.DESCRIPTION.LABEL")}<input value={form.description} placeholder={t("TEAMS_SETTINGS.FORM.DESCRIPTION.PLACEHOLDER")} onChange={e => setForm({ ...form, description: e.target.value })} className={input} /></label><label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={form.allow_auto_assign} onChange={e => setForm({ ...form, allow_auto_assign: e.target.checked })} />{t("TEAMS_SETTINGS.FORM.AUTO_ASSIGN.LABEL")}</label><div><Button type="submit" disabled={busy || form.name.trim().length < 2} label={t(editing ? "TEAMS_SETTINGS.EDIT_FLOW.CREATE.BUTTON_TEXT" : "TEAMS_SETTINGS.FORM.SUBMIT_CREATE")} /></div></form> : team && <form onSubmit={saveAgents}><table className="w-full text-left text-sm"><thead><tr className="border-b border-n-weak"><th className="p-3"><input type="checkbox" aria-label={t("TEAMS_SETTINGS.AGENTS.SELECT_ALL")} checked={agents.length > 0 && selected.length === agents.length} onChange={e => setSelected(e.target.checked ? agents.map(a => a.id) : [])} /></th><th className="p-3">{t("TEAMS_SETTINGS.AGENTS.AGENT")}</th><th className="p-3">{t("TEAMS_SETTINGS.AGENTS.EMAIL")}</th></tr></thead><tbody>{agents.map(a => <tr key={a.id} className="border-b border-n-weak"><td className="p-3"><input type="checkbox" aria-label={a.name} checked={selected.includes(a.id)} onChange={e => setSelected(e.target.checked ? [...selected, a.id] : selected.filter(id => id !== a.id))} /></td><td className="p-3">{a.name}</td><td className="p-3 text-n-slate-11">{a.email || "---"}</td></tr>)}</tbody></table><div className="mt-4 flex items-center justify-between gap-4"><p className="text-sm text-n-slate-11">{t("TEAMS_SETTINGS.AGENTS.SELECTED_COUNT", { selected: selected.length, total: agents.length })}</p><Button type="submit" disabled={busy || !selected.length} label={t(editing ? "TEAMS_SETTINGS.EDIT_FLOW.AGENTS.BUTTON_TEXT" : "TEAMS_SETTINGS.ADD.BUTTON_TEXT")} /></div></form>}
    </>}</div></div></section>;
}
