import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Condition = { attribute_key: string; filter_operator: string; values: string[]; query_operator: string; custom_attribute_type?: string };
type Action = { action_name: string; action_params: (string | number)[] };
type Rule = { id: number; name: string; description: string | null; event_name: string; active: boolean; created_on: number; conditions: Condition[]; actions: Action[]; execution_delay?: unknown };
const input = "rounded-lg border border-n-weak bg-n-background px-3 py-2";
const events = ["conversation_created", "conversation_updated", "message_created", "conversation_resolved", "conversation_opened"];
const attributes: Record<string, string> = { status: "STATUS", inbox_id: "INBOX", assignee_id: "ASSIGNEE_NAME", team_id: "TEAM_NAME", priority: "PRIORITY", labels: "LABELS", email: "EMAIL", phone_number: "PHONE_NUMBER", content: "MESSAGE_CONTAINS", message_type: "MESSAGE_TYPE", private_note: "PRIVATE_NOTE", conversation_language: "CONVERSATION_LANGUAGE", browser_language: "BROWSER_LANGUAGE", country_name: "COUNTRY_NAME", company_name: "COMPANY_NAME", referer_link: "REFERER_LINK", mail_subject: "MAIL_SUBJECT" };
const operators = ["equal_to", "not_equal_to", "contains", "does_not_contain", "is_present", "is_not_present", "starts_with"];
const actionsList = ["assign_agent", "assign_team", "remove_assigned_agent", "remove_assigned_team", "add_label", "remove_label", "send_email_to_team", "send_email_transcript", "mute_conversation", "snooze_conversation", "resolve_conversation", "open_conversation", "pending_conversation", "send_webhook_event", "send_message", "add_private_note", "change_priority"];
const noParams = ["remove_assigned_agent", "remove_assigned_team", "mute_conversation", "snooze_conversation", "resolve_conversation", "open_conversation", "pending_conversation"];
const newCondition = (): Condition => ({ attribute_key: "status", filter_operator: "equal_to", values: [], query_operator: "and", custom_attribute_type: "" });
const newAction = (): Action => ({ action_name: "assign_agent", action_params: [] });

export default function AutomationSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/automation_rules`;
  const [items, setItems] = useState<Rule[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Rule | null>(null);
  const [mode, setMode] = useState<"add" | "edit" | "delete" | "toggle" | null>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [event, setEvent] = useState(events[0]);
  const [conditions, setConditions] = useState<Condition[]>([newCondition()]);
  const [actions, setActions] = useState<Action[]>([newAction()]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function load() { const data = await api<{ payload: Rule[] }>(base); setItems(data.payload); }
  useEffect(() => { setLoading(true); load().catch(e => setError(e.message)).finally(() => setLoading(false)); }, [base]);
  function open(rule?: Rule) {
    setSelected(rule ?? null); setName(rule?.name ?? ""); setDescription(rule?.description ?? ""); setEvent(rule?.event_name ?? events[0]);
    setConditions(rule?.conditions?.length ? structuredClone(rule.conditions) : [newCondition()]); setActions(rule?.actions?.length ? structuredClone(rule.actions) : [newAction()]); setError(""); setMode(rule ? "edit" : "add");
  }
  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (!conditions.length || !actions.length) { setError(t(!conditions.length ? "AUTOMATION.ERRORS.ATLEAST_ONE_CONDITION_REQUIRED" : "AUTOMATION.ERRORS.ATLEAST_ONE_ACTION_REQUIRED")); return; }
    setBusy(true); setError("");
    try { await api(mode === "add" ? base : `${base}/${selected!.id}`, { method: mode === "add" ? "POST" : "PATCH", json: { name, description, event_name: event, conditions, actions } }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("AUTOMATION.ADD.API.ERROR_MESSAGE")); } finally { setBusy(false); }
  }
  async function mutate(rule: Rule, operation: "delete" | "toggle" | "clone") {
    setBusy(true); setError("");
    try { await api(`${base}/${rule.id}${operation === "clone" ? "/clone" : ""}`, operation === "toggle" ? { method: "PATCH", json: { active: !rule.active } } : { method: operation === "delete" ? "DELETE" : "POST" }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t(`AUTOMATION.${operation.toUpperCase()}.API.ERROR_MESSAGE`)); } finally { setBusy(false); }
  }
  function changeCondition(index: number, value: Partial<Condition>) { setConditions(conditions.map((c, i) => i === index ? { ...c, ...value } : c)); }
  function changeAction(index: number, value: Partial<Action>) { setActions(actions.map((a, i) => i === index ? { ...a, ...value } : a)); }
  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4"><div><h1 className="text-xl font-medium">{t("AUTOMATION.HEADER")}</h1><p className="mt-2 text-sm text-n-slate-11">{t("AUTOMATION.DESCRIPTION")}</p></div><Button sm label={t("AUTOMATION.HEADER_BTN_TXT")} onClick={() => open()} /></header>
    <div className="mb-4 flex justify-end"><input className={input} aria-label={t("AUTOMATION.SEARCH_PLACEHOLDER")} placeholder={t("AUTOMATION.SEARCH_PLACEHOLDER")} value={search} onChange={e => setSearch(e.target.value)} /></div>
    {error && !mode && <p role="alert" className="text-n-ruby-11">{error}</p>}
    {loading ? <p>{t("AUTOMATION.LOADING")}</p> : !items.length ? <p>{t("AUTOMATION.LIST.404")}</p> : !items.some(r => r.name.toLowerCase().includes(search.trim().toLowerCase())) ? <p>{t("AUTOMATION.NO_RESULTS")}</p> : <div className="overflow-x-auto"><table className="w-full text-left text-sm"><thead><tr className="border-b border-n-weak text-n-slate-11">{["NAME", "ACTIVE", "CREATED_ON"].map(k => <th key={k} className="p-3 font-medium">{t(`AUTOMATION.LIST.TABLE_HEADER.${k}`)}</th>)}<th className="p-3 font-medium">{t("AUTOMATION.ADD.FORM.ACTIONS.LABEL")}</th></tr></thead><tbody>{items.filter(r => r.name.toLowerCase().includes(search.trim().toLowerCase())).map(r => <tr key={r.id} className="border-b border-n-weak"><td className="p-3"><div>{r.name}</div><div className="text-n-slate-11">{r.description}</div></td><td className="p-3"><input type="checkbox" checked={r.active} aria-label={`${t("AUTOMATION.LIST.TABLE_HEADER.ACTIVE")} ${r.name}`} onChange={() => { setSelected(r); setError(""); setMode("toggle"); }} /></td><td className="p-3">{r.created_on ? new Date(r.created_on * 1000).toLocaleDateString() : ""}</td><td className="flex gap-2 p-3"><Button sm slate ghost icon="pencil" aria-label={t("AUTOMATION.FORM.EDIT")} onClick={() => open(r)} /><Button sm slate ghost icon="copy" aria-label={t("AUTOMATION.CLONE.TOOLTIP")} disabled={busy} onClick={() => mutate(r, "clone")} /><Button sm slate ghost icon="trash-2" aria-label={t("AUTOMATION.FORM.DELETE")} onClick={() => { setSelected(r); setError(""); setMode("delete"); }} /></td></tr>)}</tbody></table></div>}
    {mode && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"><div role="dialog" aria-modal="true" aria-labelledby="automation-dialog" className="max-h-full w-full max-w-2xl overflow-auto rounded-xl bg-n-background p-6 shadow-xl"><h2 id="automation-dialog" className="text-lg font-medium">{t(mode === "delete" ? "AUTOMATION.DELETE.TITLE" : mode === "toggle" ? selected?.active ? "AUTOMATION.TOGGLE.DEACTIVATION_TITLE" : "AUTOMATION.TOGGLE.ACTIVATION_TITLE" : mode === "add" ? "AUTOMATION.ADD.TITLE" : "AUTOMATION.EDIT.TITLE")}</h2>
      {mode === "delete" || mode === "toggle" ? <div className="mt-4 grid gap-4"><p>{mode === "delete" ? `${t("AUTOMATION.DELETE.CONFIRM.MESSAGE")}${selected?.name}?` : t(selected?.active ? "AUTOMATION.TOGGLE.DEACTIVATION_DESCRIPTION" : "AUTOMATION.TOGGLE.ACTIVATION_DESCRIPTION", { automationName: selected?.name ?? "" })}</p>{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="flex justify-end gap-2"><Button slate faded label={t(mode === "delete" ? "AUTOMATION.DELETE.CANCEL_BUTTON_TEXT" : "AUTOMATION.TOGGLE.CANCEL_LABEL")} onClick={() => setMode(null)} /><Button ruby={mode === "delete"} disabled={busy} label={t(mode === "delete" ? "AUTOMATION.DELETE.SUBMIT" : "AUTOMATION.TOGGLE.CONFIRMATION_LABEL")} onClick={() => selected && mutate(selected, mode)} /></div></div> : <form className="mt-4 grid gap-4" onSubmit={save}>
        <label className="grid gap-1 text-sm">{t("AUTOMATION.ADD.FORM.NAME.LABEL")}<input required className={input} value={name} onChange={e => setName(e.target.value)} placeholder={t("AUTOMATION.ADD.FORM.NAME.PLACEHOLDER")} /></label>
        <label className="grid gap-1 text-sm">{t("AUTOMATION.ADD.FORM.DESC.LABEL")}<input required className={input} value={description} onChange={e => setDescription(e.target.value)} placeholder={t("AUTOMATION.ADD.FORM.DESC.PLACEHOLDER")} /></label>
        <label className="grid gap-1 text-sm">{t("AUTOMATION.ADD.FORM.EVENT.LABEL")}<select className={input} value={event} onChange={e => setEvent(e.target.value)}>{!events.includes(event) && <option value={event}>{event}</option>}{events.map(v => <option key={v} value={v}>{t(`AUTOMATION.EVENTS.${v.toUpperCase()}`)}</option>)}</select></label>
        <p>{t("AUTOMATION.ADD.FORM.CONDITIONS.LABEL")}</p>{conditions.map((c, i) => <div key={i} className="grid gap-2 rounded-lg border border-n-weak p-3"><select className={input} aria-label={t("AUTOMATION.ERRORS.ATTRIBUTE_KEY_REQUIRED")} value={c.attribute_key} onChange={e => changeCondition(i, { attribute_key: e.target.value, values: [] })}>{!attributes[c.attribute_key] && <option value={c.attribute_key}>{c.attribute_key}</option>}{Object.entries(attributes).map(([key, label]) => <option key={key} value={key}>{t(`AUTOMATION.ATTRIBUTES.${label}`)}</option>)}</select><select className={input} aria-label={t("AUTOMATION.ERRORS.FILTER_OPERATOR_REQUIRED")} value={c.filter_operator} onChange={e => changeCondition(i, { filter_operator: e.target.value })}>{!operators.includes(c.filter_operator) && <option value={c.filter_operator}>{c.filter_operator}</option>}{operators.map(op => <option key={op} value={op}>{op.replaceAll("_", " ")}</option>)}</select>{!["is_present", "is_not_present"].includes(c.filter_operator) && <input required className={input} aria-label={t("AUTOMATION.ERRORS.VALUE_REQUIRED")} value={Array.isArray(c.values) ? c.values.join(", ") : c.values || ""} onChange={e => changeCondition(i, { values: e.target.value.split(",").map(v => v.trimStart()) })} />}{i < conditions.length - 1 && <select className={input} aria-label={t("AUTOMATION.ADD.FORM.CONDITIONS.LABEL")} value={c.query_operator || "and"} onChange={e => changeCondition(i, { query_operator: e.target.value })}><option value="and">AND</option><option value="or">OR</option></select>}<Button sm slate faded type="button" label={t("AUTOMATION.FORM.DELETE")} disabled={conditions.length === 1} onClick={() => setConditions(conditions.filter((_, n) => n !== i))} /></div>)}<Button sm slate faded type="button" label={t("AUTOMATION.ADD.CONDITION_BUTTON_LABEL")} onClick={() => setConditions([...conditions, newCondition()])} />
        <p>{t("AUTOMATION.ADD.FORM.ACTIONS.LABEL")}</p>{actions.map((a, i) => <div key={i} className="grid gap-2 rounded-lg border border-n-weak p-3"><select className={input} aria-label={t("AUTOMATION.ADD.FORM.ACTIONS.LABEL")} value={a.action_name} onChange={e => changeAction(i, { action_name: e.target.value, action_params: [] })}>{!actionsList.includes(a.action_name) && <option value={a.action_name}>{a.action_name}</option>}{actionsList.map(v => <option key={v} value={v}>{t(`AUTOMATION.ACTIONS.${v.toUpperCase()}`)}</option>)}</select>{!noParams.includes(a.action_name) && <input required className={input} aria-label={t("AUTOMATION.ERRORS.ACTION_PARAMETERS_REQUIRED")} value={(a.action_params ?? []).join(", ")} onChange={e => changeAction(i, { action_params: e.target.value.split(",").map(v => v.trimStart()) })} />}<Button sm slate faded type="button" label={t("AUTOMATION.FORM.DELETE")} disabled={actions.length === 1} onClick={() => setActions(actions.filter((_, n) => n !== i))} /></div>)}<Button sm slate faded type="button" label={t("AUTOMATION.ADD.ACTION_BUTTON_LABEL")} onClick={() => setActions([...actions, newAction()])} />
        {error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="flex justify-end gap-2"><Button slate faded type="button" label={t("AUTOMATION.FORM.CANCEL")} onClick={() => setMode(null)} /><Button type="submit" disabled={busy} label={t(mode === "add" ? "AUTOMATION.ADD.SUBMIT" : "AUTOMATION.EDIT.SUBMIT")} /></div>
      </form>}</div></div>}
  </section>;
}
