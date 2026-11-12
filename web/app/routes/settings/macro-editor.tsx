import { useEffect, useState } from "react";
import { useNavigate, useOutletContext, useParams } from "react-router";
import Button from "~/components/next/Button";
import { acct, api, type Agent, type Label, type Team } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Value = string | number;
type Action = { action_name: string; action_params: Value[] };
type Macro = { name: string; visibility: "personal" | "global"; actions: Action[]; files?: { blob_id: number; filename: string }[] };
type Option = { id: Value; name: string };
const types = ["assign_team", "assign_agent", "add_label", "remove_label", "remove_assigned_agent", "remove_assigned_team", "send_email_transcript", "mute_conversation", "snooze_conversation", "resolve_conversation", "send_attachment", "send_message", "add_private_note", "change_priority", "send_webhook_event"];
const noParams = ["remove_assigned_agent", "remove_assigned_team", "mute_conversation", "snooze_conversation", "resolve_conversation"];
const input = "w-full rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm";
const list = <T,>(data: T[] | { payload: T[] }): T[] => Array.isArray(data) ? data : data.payload;

export default function MacroEditor() {
  const { accountId, me } = useOutletContext<ShellCtx>();
  const { macroId } = useParams();
  const navigate = useNavigate();
  const base = acct(accountId);
  const admin = me.accounts?.some(a => a.id === accountId && a.role === "administrator") ?? false;
  const [macro, setMacro] = useState<Macro>({ name: "", visibility: admin ? "global" : "personal", actions: [{ action_name: "assign_team", action_params: [] }] });
  const [agents, setAgents] = useState<Agent[]>([]);
  const [teams, setTeams] = useState<Team[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const readOnly = !!macroId && macro.visibility === "global" && !admin;

  useEffect(() => {
    let active = true;
    setLoading(true);
    Promise.all([
      api<Agent[] | { payload: Agent[] }>(`${base}/agents`),
      api<Team[] | { payload: Team[] }>(`${base}/teams`),
      api<Label[] | { payload: Label[] }>(`${base}/labels`),
      macroId ? api<{ payload: Macro }>(`${base}/macros/${macroId}`) : Promise.resolve(null),
    ]).then(([a, b, c, m]) => {
      if (!active) return;
      setAgents(list(a)); setTeams(list(b)); setLabels(list(c));
      if (m) setMacro(m.payload);
    }).catch(e => { if (active) setError(e instanceof Error ? e.message : t("MACROS.ERROR")); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [base, macroId]);

  function update(index: number, action: Action) {
    setMacro(m => ({ ...m, actions: m.actions.map((a, i) => i === index ? action : a) }));
  }
  function move(index: number, delta: number) {
    setMacro(m => { const actions = [...m.actions]; const next = index + delta; if (next < 0 || next >= actions.length) return m; [actions[index], actions[next]] = [actions[next], actions[index]]; return { ...m, actions }; });
  }
  function options(name: string): Option[] | null {
    if (name === "assign_agent") return [{ id: "nil", name: "None" }, { id: "self", name: "Self" }, ...agents.map(a => ({ id: a.id, name: a.name }))];
    if (name === "assign_team") return [{ id: "nil", name: "None" }, ...teams.map(a => ({ id: a.id, name: a.name }))];
    if (name === "add_label" || name === "remove_label") return labels.map(a => ({ id: a.title, name: a.title }));
    if (name === "change_priority") return ["nil", "low", "medium", "high", "urgent"].map(id => ({ id, name: t(`MACROS.PRIORITY_TYPES.${id === "nil" ? "NONE" : id.toUpperCase()}`) }));
    return null;
  }
  async function upload(index: number, file: File) {
    setBusy(true); setError("");
    try {
      const body = new FormData(); body.append("attachment", file);
      const result = await api<{ blob_id: number }>(`${base}/upload`, { method: "POST", body });
      setMacro(m => ({ ...m, actions: m.actions.map((a, i) => i === index ? { ...a, action_params: [result.blob_id] } : a), files: [...(m.files ?? []), { blob_id: result.blob_id, filename: file.name }] }));
    } catch (e) { setError(e instanceof Error ? e.message : t("AUTOMATION.ATTACHMENT.UPLOAD_ERROR")); }
    finally { setBusy(false); }
  }
  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (readOnly || busy) return;
    if (!macro.name.trim() || !macro.actions.length || macro.actions.some(a => !noParams.includes(a.action_name) && (!a.action_params.length || a.action_params.some(v => String(v).trim() === "")))) { setError(t("MACROS.ERRORS.ACTION_PARAMETERS_REQUIRED")); return; }
    setBusy(true); setError("");
    try {
      await api(`${base}/macros${macroId ? `/${macroId}` : ""}`, { method: macroId ? "PATCH" : "POST", json: { name: macro.name.trim(), visibility: macro.visibility, actions: macro.actions } });
      navigate(`/accounts/${accountId}/settings/macros`);
    } catch (err) { setError(err instanceof Error ? err.message : t("MACROS.ERROR")); }
    finally { setBusy(false); }
  }

  if (loading) return <p className="p-8">{t("MACROS.EDITOR.LOADING")}</p>;
  if (error && macroId && !macro.name) return <p role="alert" className="p-8 text-n-ruby-11">{error}</p>;
  return <form onSubmit={save} className="mx-auto flex h-full w-full max-w-7xl flex-col gap-6 overflow-auto p-6 text-n-slate-12 lg:flex-row">
    <fieldset disabled={busy || readOnly} className="flex-1 space-y-4" aria-label={t("MACROS.ADD.FORM.ACTIONS.LABEL")}>
      <p className="text-sm text-n-slate-11">{t("MACROS.EDITOR.START_FLOW")}</p>
      {macro.actions.map((action, index) => {
        const choices = options(action.action_name);
        const multi = action.action_name === "add_label" || action.action_name === "remove_label";
        return <div key={index} className="flex items-start gap-2">
          <div className="grid gap-1"><button type="button" className="rounded border border-n-weak px-2" aria-label="Move action up" disabled={readOnly || index === 0} onClick={() => move(index, -1)}>↑</button><button type="button" className="rounded border border-n-weak px-2" aria-label="Move action down" disabled={readOnly || index === macro.actions.length - 1} onClick={() => move(index, 1)}>↓</button></div>
          <div className="grid flex-1 gap-2 rounded-lg border border-n-weak bg-n-background p-3">
            <select className={input} aria-label={t("MACROS.ADD.FORM.ACTIONS.LABEL")} disabled={readOnly} value={action.action_name} onChange={e => update(index, { action_name: e.target.value, action_params: [] })}>
              {!types.includes(action.action_name) && <option value={action.action_name}>{action.action_name}</option>}{types.map(type => <option key={type} value={type}>{t(`MACROS.ACTIONS.${type.toUpperCase()}`)}</option>)}
            </select>
            {choices && <select className={input} aria-label={t("MACROS.ERRORS.VALUE_REQUIRED")} disabled={readOnly} multiple={multi} required value={multi ? action.action_params.map(String) : String(action.action_params[0] ?? "")} onChange={e => { const values = Array.from(e.target.selectedOptions).map(o => choices.find(c => String(c.id) === o.value)?.id).filter((v): v is Value => v !== undefined); update(index, { ...action, action_params: values }); }}>
              {!multi && <option value="">{t("MACROS.ERRORS.VALUE_REQUIRED")}</option>}{choices.map(c => <option key={c.id} value={String(c.id)}>{c.name}</option>)}
            </select>}
            {!choices && !noParams.includes(action.action_name) && (action.action_name === "send_attachment" ? <label className="text-sm">{t("AUTOMATION.ATTACHMENT.LABEL_IDLE")}<input className={input} type="file" disabled={readOnly || busy} onChange={e => { const file = e.target.files?.[0]; if (file) void upload(index, file); }} />{macro.files?.find(f => f.blob_id === action.action_params[0])?.filename}</label> : action.action_name === "send_message" || action.action_name === "add_private_note" ? <textarea className={input} aria-label={t("MACROS.ERRORS.VALUE_REQUIRED")} required readOnly={readOnly} value={String(action.action_params[0] ?? "")} onChange={e => update(index, { ...action, action_params: [e.target.value] })} /> : <input className={input} aria-label={t("MACROS.ERRORS.VALUE_REQUIRED")} type={action.action_name === "send_email_transcript" ? "email" : action.action_name === "send_webhook_event" ? "url" : "text"} required readOnly={readOnly} value={String(action.action_params[0] ?? "")} onChange={e => update(index, { ...action, action_params: [e.target.value] })} />)}
          </div>
          <Button type="button" sm slate faded label={t("MACROS.EDITOR.DELETE_BTN_TOOLTIP")} disabled={readOnly || macro.actions.length === 1} onClick={() => setMacro(m => ({ ...m, actions: m.actions.filter((_, i) => i !== index) }))} />
        </div>;
      })}
      <Button type="button" sm slate faded label={t("MACROS.EDITOR.ADD_BTN_TOOLTIP")} disabled={readOnly} onClick={() => setMacro(m => ({ ...m, actions: [...m.actions, { action_name: "assign_team", action_params: [] }] }))} />
      <p className="text-sm text-n-slate-11">{t("MACROS.EDITOR.END_FLOW")}</p>
    </fieldset>
    <aside className="h-fit w-full space-y-4 rounded-lg border border-n-weak bg-n-solid-2 p-4 lg:w-1/3">
      <label className="grid gap-1 text-sm">{t("MACROS.ADD.FORM.NAME.LABEL")}<input className={input} required readOnly={readOnly} placeholder={t("MACROS.ADD.FORM.NAME.PLACEHOLDER")} value={macro.name} onChange={e => setMacro({ ...macro, name: e.target.value })} /></label>
      <fieldset disabled={readOnly} className="grid gap-2"><legend className="text-sm font-medium">{t("MACROS.EDITOR.VISIBILITY.LABEL")}</legend>{(["global", "personal"] as const).map(v => <label key={v} className="flex gap-2 rounded-lg border border-n-weak p-3 text-sm"><input type="radio" name="visibility" value={v} checked={macro.visibility === v} disabled={v === "global" && !admin} onChange={() => setMacro({ ...macro, visibility: v })} /><span><strong>{t(`MACROS.EDITOR.VISIBILITY.${v.toUpperCase()}.LABEL`)}</strong><span className="block text-n-slate-11">{t(`MACROS.EDITOR.VISIBILITY.${v.toUpperCase()}.${v === "global" && !admin ? readOnly ? "EDIT_DISABLED_DESCRIPTION" : "CREATE_DISABLED_DESCRIPTION" : "DESCRIPTION"}`)}</span></span></label>)}</fieldset>
      <p className="text-sm text-n-slate-11">{t("MACROS.ORDER_INFO")}</p>
      {error && <p role="alert" className="text-sm text-n-ruby-11">{error}</p>}
      <Button type="submit" label={t("MACROS.HEADER_BTN_TXT_SAVE")} disabled={readOnly || busy} />
    </aside>
  </form>;
}
