import { useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useOutletContext, useParams } from "react-router";
import { Button, ErrorText, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Policy = { id: number; name: string; description: string | null; enabled: boolean; assignment_order: string; conversation_priority: string; fair_distribution_limit: number; fair_distribution_window: number; exclude_older_than_hours: number | null; assigned_inbox_count: number };
type Inbox = { id: number; name: string };
const P = "ASSIGNMENT_POLICY.AGENT_ASSIGNMENT_POLICY";
const initial: Omit<Policy, "id" | "assigned_inbox_count"> = { name: "", description: "", enabled: true, assignment_order: "round_robin", conversation_priority: "earliest_created", fair_distribution_limit: 100, fair_distribution_window: 3600, exclude_older_than_hours: 168 };
const input = "w-full rounded border border-line bg-bg px-3 py-2 text-sm";
const errorMessage = (e: unknown) => e instanceof Error ? e.message : "Request failed";

export default function AssignmentPolicy() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { id } = useParams();
  const path = useLocation().pathname;
  const navigate = useNavigate();
  const root = `/accounts/${accountId}/settings/assignment-policy`;
  const base = `${acct(accountId)}/assignment_policies`;
  const edit = !!id;
  const formPage = edit || path.endsWith("/create");
  const listPage = path.endsWith("/assignment");
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [form, setForm] = useState(initial);
  const [inboxes, setInboxes] = useState<Inbox[]>([]);
  const [linked, setLinked] = useState<Inbox[]>([]);
  const [selected, setSelected] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function loadLinked() { if (id) setLinked((await api<{inboxes: Inbox[]}>(`${base}/${id}/inboxes`)).inboxes); }
  useEffect(() => {
    setError("");
    if (listPage) api<Policy[]>(base).then(setPolicies).catch(e => setError(errorMessage(e)));
    if (edit) {
      api<Policy>(`${base}/${id}`).then(setForm).catch(e => setError(errorMessage(e)));
      loadLinked().catch(e => setError(errorMessage(e)));
      api<{payload: Inbox[]} | Inbox[]>(`${acct(accountId)}/inboxes`).then(r => setInboxes(Array.isArray(r) ? r : r.payload)).catch(e => setError(errorMessage(e)));
    } else setForm(initial);
  }, [accountId, base, id, listPage, edit]);
  async function save(e: React.FormEvent) {
    e.preventDefault(); setBusy(true); setError("");
    try { const p = await api<Policy>(edit ? `${base}/${id}` : base, { method: edit ? "PATCH" : "POST", json: { assignment_policy: form } }); navigate(`${root}/assignment/edit/${p.id}`); }
    catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  }
  async function linkInbox(inboxId: number, remove = false) {
    setBusy(true); setError("");
    try {
      if (!remove) {
        try { const previous = await api<Policy>(`${acct(accountId)}/inboxes/${inboxId}/assignment_policy`); if (previous.id !== Number(id) && !window.confirm(t(`${P}.EDIT.CONFIRM_ADD_INBOX_DIALOG.DESCRIPTION`).replace("{inboxName}", inboxes.find(i => i.id === inboxId)?.name ?? ""))) return; } catch (e) { if (!(e instanceof Error && "status" in e && e.status === 404)) throw e; }
      }
      await api(`${acct(accountId)}/inboxes/${inboxId}/assignment_policy`, remove ? { method: "DELETE" } : { method: "POST", json: { assignment_policy_id: Number(id) } });
      await loadLinked(); setSelected("");
    } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  }
  async function removePolicy(p: Policy) {
    if (!window.confirm(`Delete ${p.name}?`)) return;
    try { await api(`${base}/${p.id}`, { method: "DELETE" }); setPolicies(await api<Policy[]>(base)); } catch(e) { setError(errorMessage(e)); }
  }
  if (!formPage && !listPage) return <section className="flex min-w-0 flex-1 flex-col"><PageHeader title={t("ASSIGNMENT_POLICY.INDEX.HEADER.TITLE")} /><div className="p-6"><p className="mb-6 text-sm text-ink-3">{t("ASSIGNMENT_POLICY.INDEX.HEADER.DESCRIPTION")}</p><Link className="block max-w-lg rounded-lg border border-line p-4" to={`${root}/assignment`}><h2 className="font-medium">{t("ASSIGNMENT_POLICY.INDEX.ASSIGNMENT_POLICY.TITLE")}</h2><p className="text-sm text-ink-3">{t("ASSIGNMENT_POLICY.INDEX.ASSIGNMENT_POLICY.DESCRIPTION")}</p></Link></div></section>;
  if (listPage) return <section className="flex min-w-0 flex-1 flex-col"><PageHeader title={t(`${P}.INDEX.HEADER.TITLE`)}><Link className="rounded bg-woot-500 px-4 py-2 text-sm text-white" to={`${root}/assignment/create`}>{t(`${P}.INDEX.HEADER.CREATE_POLICY`)}</Link></PageHeader><div className="space-y-4 p-6"><ErrorText>{error}</ErrorText>{policies.length === 0 && <p>{t(`${P}.INDEX.NO_RECORDS_FOUND`)}</p>}{policies.map(p => <article key={p.id} className="rounded-lg border border-line p-4"><div className="flex justify-between gap-4"><div><h2 className="font-medium">{p.name}</h2><p className="text-sm text-ink-3">{p.description}</p><p className="mt-2 text-sm">{t(`${P}.INDEX.CARD.ORDER`)}: {p.assignment_order === "round_robin" ? t(`${P}.FORM.ASSIGNMENT_ORDER.ROUND_ROBIN.LABEL`) : t(`${P}.FORM.ASSIGNMENT_ORDER.BALANCED.LABEL`)} · {t(`${P}.INDEX.CARD.PRIORITY`)}: {p.conversation_priority === "earliest_created" ? t(`${P}.FORM.ASSIGNMENT_PRIORITY.EARLIEST_CREATED.LABEL`) : t(`${P}.FORM.ASSIGNMENT_PRIORITY.LONGEST_WAITING.LABEL`)} · {p.enabled ? t(`${P}.INDEX.CARD.ACTIVE`) : t(`${P}.INDEX.CARD.INACTIVE`)} · {t(`${P}.INDEX.CARD.POPOVER`)}: {p.assigned_inbox_count}</p></div><div><Link className="text-sm underline" to={`${root}/assignment/edit/${p.id}`}>{t(`${P}.INDEX.CARD.EDIT`)}</Link><Button variant="danger" onClick={() => removePolicy(p)}>Delete</Button></div></div></article>)}</div></section>;
  return <section className="flex min-w-0 flex-1 flex-col"><PageHeader title={t(`${P}.${edit ? "EDIT" : "CREATE"}.HEADER.TITLE`)} /><form onSubmit={save} className="w-full max-w-2xl space-y-5 overflow-auto p-6"><ErrorText>{error}</ErrorText><label className="block text-sm">{t(`${P}.FORM.NAME.LABEL`)}<input className={input} required value={form.name} placeholder={t(`${P}.FORM.NAME.PLACEHOLDER`)} onChange={e => setForm({...form,name:e.target.value})} /></label><label className="block text-sm">{t(`${P}.FORM.DESCRIPTION.LABEL`)}<textarea className={input} value={form.description ?? ""} placeholder={t(`${P}.FORM.DESCRIPTION.PLACEHOLDER`)} onChange={e => setForm({...form,description:e.target.value})} /></label><label className="block text-sm">{t(`${P}.FORM.STATUS.LABEL`)}<select className={input} value={String(form.enabled)} onChange={e => setForm({...form,enabled:e.target.value === "true"})}><option value="true">{t(`${P}.FORM.STATUS.ACTIVE`)}</option><option value="false">{t(`${P}.FORM.STATUS.INACTIVE`)}</option></select></label><fieldset><legend>{t(`${P}.FORM.ASSIGNMENT_ORDER.LABEL`)}</legend>{["round_robin", "balanced"].map(v => <label key={v} className="block text-sm"><input type="radio" name="order" checked={form.assignment_order === v} disabled={v === "balanced"} onChange={() => setForm({...form,assignment_order:v})} /> {t(`${P}.FORM.ASSIGNMENT_ORDER.${v.toUpperCase()}.LABEL`)}: {t(`${P}.FORM.ASSIGNMENT_ORDER.${v.toUpperCase()}.DESCRIPTION`)}</label>)}</fieldset><fieldset><legend>{t(`${P}.FORM.ASSIGNMENT_PRIORITY.LABEL`)}</legend>{["earliest_created", "longest_waiting"].map(v => <label key={v} className="block text-sm"><input type="radio" name="priority" checked={form.conversation_priority === v} onChange={() => setForm({...form,conversation_priority:v})} /> {t(`${P}.FORM.ASSIGNMENT_PRIORITY.${v.toUpperCase()}.LABEL`)}: {t(`${P}.FORM.ASSIGNMENT_PRIORITY.${v.toUpperCase()}.DESCRIPTION`)}</label>)}</fieldset><fieldset><legend>{t(`${P}.FORM.FAIR_DISTRIBUTION.LABEL`)}</legend><p className="text-sm text-ink-3">{t(`${P}.FORM.FAIR_DISTRIBUTION.DESCRIPTION`)}</p><label className="block text-sm">{t(`${P}.FORM.FAIR_DISTRIBUTION.INPUT_MAX`)}<input className={input} type="number" min="1" required value={form.fair_distribution_limit} onChange={e => setForm({...form,fair_distribution_limit:Number(e.target.value)})} /></label><label className="block text-sm">{t(`${P}.FORM.FAIR_DISTRIBUTION.DURATION`)}<input className={input} type="number" min="1" required value={form.fair_distribution_window} onChange={e => setForm({...form,fair_distribution_window:Number(e.target.value)})} /></label></fieldset><label className="block text-sm">{t(`${P}.FORM.EXCLUDE_OLDER_THAN.LABEL`)}<p className="text-ink-3">{t(`${P}.FORM.EXCLUDE_OLDER_THAN.DESCRIPTION`)}</p><input className={input} type="number" min="1" value={form.exclude_older_than_hours ?? ""} onChange={e => setForm({...form,exclude_older_than_hours:e.target.value ? Number(e.target.value) : null})} /></label><Button type="submit" disabled={busy}>{t(`${P}.${edit ? "EDIT.EDIT_BUTTON" : "CREATE.CREATE_BUTTON"}`)}</Button>{edit && <div><h2>{t(`${P}.FORM.INBOXES.LABEL`)}</h2><p className="text-sm text-ink-3">{t(`${P}.FORM.INBOXES.DESCRIPTION`)}</p>{linked.length === 0 && <p>{t(`${P}.FORM.INBOXES.EMPTY_STATE`)}</p>}{linked.map(i => <div key={i.id} className="flex items-center justify-between border-b border-line py-2">{i.name}<Button type="button" variant="quiet" disabled={busy} onClick={() => linkInbox(i.id,true)}>Remove</Button></div>)}<label className="block text-sm">{t(`${P}.FORM.INBOXES.ADD_BUTTON`)}<select className={input} value={selected} onChange={e => setSelected(e.target.value)}><option value="">{t(`${P}.FORM.INBOXES.DROPDOWN.SEARCH_PLACEHOLDER`)}</option>{inboxes.filter(i => !linked.some(l => l.id === i.id)).map(i => <option key={i.id} value={i.id}>{i.name}</option>)}</select></label><Button type="button" disabled={!selected || busy} onClick={() => linkInbox(Number(selected))}>{t(`${P}.FORM.INBOXES.DROPDOWN.ADD_BUTTON`)}</Button></div>}</form></section>;
}
