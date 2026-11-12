import { useEffect, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Inbox = {
  id: number; name: string; channel_type: string; website_url?: string | null; webhook_url?: string | null;
  widget_color?: string | null; welcome_title?: string | null; welcome_tagline?: string | null;
  reply_time?: string | null; greeting_enabled?: boolean | null; greeting_message?: string | null;
  enable_email_collect?: boolean | null; enable_auto_assignment?: boolean | null;
  allow_messages_after_resolved?: boolean | null; lock_to_single_conversation?: boolean | null;
  inbox_identifier?: string | null; hmac_token?: string | null; hmac_mandatory?: boolean | null;
  web_widget_script?: string | null;
};
type Agent = { id: number; name: string; email: string };
const input = "w-full rounded-lg border border-n-weak bg-n-background px-3 py-2";
const popup = "INBOX_MGMT.SETTINGS_POPUP.";
const website = "INBOX_MGMT.ADD.WEBSITE_CHANNEL.";

export default function InboxesSettings() {
  const { accountId, me } = useOutletContext<ShellCtx>();
  const { inboxId, tab } = useParams();
  const root = `${acct(accountId)}`;
  const path = `/accounts/${accountId}/settings/inboxes`;
  const isAdmin = me.accounts?.some(a => a.id === accountId && a.role === "administrator");
  const [inboxes, setInboxes] = useState<Inbox[]>([]);
  const [inbox, setInbox] = useState<Inbox | null>(null);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [agentsReady, setAgentsReady] = useState(false);
  const [selected, setSelected] = useState<number[]>([]);
  const [search, setSearch] = useState("");
  const [deleting, setDeleting] = useState<Inbox | null>(null);
  const [confirm, setConfirm] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const currentTab = ["collaborators", "configuration"].includes(tab ?? "") ? tab : "inbox-settings";
  async function loadList() { const result = await api<{ payload: Inbox[] }>(`${root}/inboxes`); setInboxes(result.payload); }
  useEffect(() => {
    let active = true;
    setLoading(true); setError(""); setNotice(""); setInbox(null);
    const request = inboxId ? api<Inbox>(`${root}/inboxes/${inboxId}`).then(data => { if (active) setInbox(data); }) : api<{ payload: Inbox[] }>(`${root}/inboxes`).then(data => { if (active) setInboxes(data.payload); });
    request.catch(e => { if (active) setError(e.message); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [root, inboxId]);
  useEffect(() => {
    if (!inboxId || currentTab !== "collaborators") return;
    let active = true;
    setAgentsReady(false);
    Promise.all([api<Agent[]>(`${root}/agents`), api<{ payload: Agent[] }>(`${root}/inbox_members/${inboxId}`)])
      .then(([all, members]) => { if (active) { setAgents(all); setSelected(members.payload.map(a => a.id)); setAgentsReady(true); } })
      .catch(e => { if (active) setError(e.message); });
    return () => { active = false; };
  }, [root, inboxId, currentTab]);
  function update<K extends keyof Inbox>(key: K, value: Inbox[K]) { setInbox(old => old ? { ...old, [key]: value } : old); }
  async function patch(data: object) {
    if (!inbox) return;
    setBusy(true); setError(""); setNotice("");
    try { const saved = await api<Inbox>(`${root}/inboxes/${inbox.id}`, { method: "PATCH", json: data }); setInbox(saved); setNotice(t("INBOX_MGMT.EDIT.API.SUCCESS_MESSAGE")); }
    catch (e) { setError(e instanceof Error ? e.message : t("INBOX_MGMT.EDIT.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  async function saveSettings(e: React.FormEvent) {
    e.preventDefault(); if (!inbox) return;
    await patch({ name: inbox.name.trim(), greeting_enabled: !!inbox.greeting_enabled, greeting_message: inbox.greeting_message ?? "", enable_email_collect: !!inbox.enable_email_collect, allow_messages_after_resolved: !!inbox.allow_messages_after_resolved, lock_to_single_conversation: !!inbox.lock_to_single_conversation, channel: inbox.channel_type === "Channel::Api" ? { webhook_url: inbox.webhook_url ?? "" } : { website_url: inbox.website_url ?? "", widget_color: inbox.widget_color ?? "", welcome_title: inbox.welcome_title ?? "", welcome_tagline: inbox.welcome_tagline ?? "", reply_time: inbox.reply_time ?? "in_a_few_minutes" } });
  }
  async function saveAgents() {
    if (!inbox) return;
    setBusy(true); setError(""); setNotice("");
    try { await api(`${root}/inbox_members`, { method: "POST", json: { inbox_id: inbox.id, user_ids: selected } }); setNotice(t("AGENT_MGMT.EDIT.API.SUCCESS_MESSAGE")); }
    catch (e) { setError(e instanceof Error ? e.message : t("AGENT_MGMT.EDIT.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  async function remove() {
    if (!deleting) return;
    setBusy(true); setError("");
    try { await api(`${root}/inboxes/${deleting.id}`, { method: "DELETE" }); await loadList(); setDeleting(null); setNotice(t("INBOX_MGMT.DELETE.API.SUCCESS_MESSAGE")); }
    catch (e) { setError(e instanceof Error ? e.message : t("INBOX_MGMT.DELETE.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }
  const field = (key: keyof Inbox, label: string, type = "text") => <label className="grid gap-2 text-sm">{t(label)}<input className={input} type={type} value={String(inbox?.[key] ?? "")} onChange={e => update(key, e.target.value)} /></label>;
  const toggle = (key: keyof Inbox, label: string, description?: string) => <label className="flex items-start gap-3 border-t border-n-weak py-4 text-sm"><input type="checkbox" checked={!!inbox?.[key]} onChange={e => update(key, e.target.checked)} /><span>{t(label)}{description && <span className="mt-1 block text-n-slate-11">{t(description)}</span>}</span></label>;
  const web = inbox?.channel_type === "Channel::WebWidget";
  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    {loading ? <p>{t("GENERAL.LOADING")}</p> : !inboxId ? <>
      <header className="mb-6 flex flex-wrap items-start justify-between gap-4"><div><h1 className="text-xl font-medium">{t("INBOX_MGMT.HEADER")}</h1><p className="mt-2 max-w-3xl text-sm text-n-slate-11">{t("INBOX_MGMT.DESCRIPTION")}</p></div>{isAdmin && <Link to={`${path}/new`}><Button sm label={t("SETTINGS.INBOXES.NEW_INBOX")} /></Link>}</header>
      <div className="mb-4 flex justify-end"><input className={input + " max-w-sm"} aria-label={t("INBOX_MGMT.SEARCH_PLACEHOLDER")} placeholder={t("INBOX_MGMT.SEARCH_PLACEHOLDER")} value={search} onChange={e => setSearch(e.target.value)} /></div>
      {inboxes.length === 0 ? <p>{t("INBOX_MGMT.LIST.404")}</p> : !inboxes.some(i => `${i.name} ${i.channel_type} ${i.website_url ?? ""}`.toLowerCase().includes(search.toLowerCase().trim())) ? <p>{t("INBOX_MGMT.NO_RESULTS")}</p> : inboxes.filter(i => `${i.name} ${i.channel_type} ${i.website_url ?? ""}`.toLowerCase().includes(search.toLowerCase().trim())).sort((a, b) => a.name.localeCompare(b.name)).map(i => <div key={i.id} className="flex items-center justify-between gap-4 border-t border-n-weak py-4"><div><div className="font-medium">{i.name}</div><div className="text-sm text-n-slate-11">{t(i.channel_type === "Channel::WebWidget" ? "INBOX_MGMT.CHANNELS.WEBSITE" : i.channel_type === "Channel::Api" ? "INBOX_MGMT.CHANNELS.API" : "INBOX_MGMT.HEADER")}{i.website_url ? ` ${i.website_url}` : ""}</div></div><div className="flex gap-2"><Link to={`/accounts/${accountId}/inbox/${i.id}`}><Button sm slate icon="messages-square" aria-label={t("INBOX_MGMT.VIEW_CONVERSATIONS")} /></Link>{isAdmin && <><Link to={`${path}/${i.id}`}><Button sm slate icon="settings" aria-label={t("INBOX_MGMT.SETTINGS")} /></Link><Button sm slate icon="trash-2" aria-label={t("INBOX_MGMT.DELETE.BUTTON_TEXT")} onClick={() => { setDeleting(i); setConfirm(""); setError(""); }} /></>}</div></div>)}
    </> : inbox && <>
      <Link to={`${path}/list`} className="text-sm text-n-slate-11">{t("INBOX_MGMT.HEADER")}</Link><h1 className="mt-2 text-xl font-medium">{inbox.name}</h1>
      <nav className="my-6 flex flex-wrap gap-6 border-b border-n-weak" aria-label={t("INBOX_MGMT.HEADER")}>{([ ["inbox-settings", "SETTINGS"], ["collaborators", "COLLABORATORS"], ["configuration", "CONFIGURATION"] ] as const).map(([key, label]) => <Link key={key} to={`${path}/${inbox.id}/${key}`} aria-current={currentTab === key ? "page" : undefined} className={`border-b-2 pb-3 text-sm ${currentTab === key ? "border-n-blue-9 text-n-blue-11" : "border-transparent text-n-slate-11"}`}>{t(`INBOX_MGMT.TABS.${label}`)}</Link>)}</nav>
      {currentTab === "inbox-settings" ? <form onSubmit={saveSettings} className="grid max-w-2xl gap-5">{field("name", web ? "INBOX_MGMT.ADD.WEBSITE_NAME.LABEL" : "INBOX_MGMT.ADD.CHANNEL_NAME.LABEL")}{web ? <>{field("website_url", website + "CHANNEL_DOMAIN.LABEL")}{field("welcome_title", website + "CHANNEL_WELCOME_TITLE.LABEL")}{field("welcome_tagline", website + "CHANNEL_WELCOME_TAGLINE.LABEL")}{field("widget_color", website + "WIDGET_COLOR.LABEL")}<label className="grid gap-2 text-sm">{t(website + "REPLY_TIME.TITLE")}<select className={input} value={inbox.reply_time ?? "in_a_few_minutes"} onChange={e => update("reply_time", e.target.value)}>{["in_a_few_minutes", "in_a_few_hours", "in_a_day"].map(value => <option key={value} value={value}>{t(website + "REPLY_TIME." + value.toUpperCase())}</option>)}</select></label></> : inbox.channel_type === "Channel::Api" && field("webhook_url", website + "CHANNEL_WEBHOOK_URL.LABEL", "url")}
        {toggle("greeting_enabled", website + "CHANNEL_GREETING_TOGGLE.LABEL", website + "CHANNEL_GREETING_TOGGLE.HELP_TEXT")}{inbox.greeting_enabled && <label className="grid gap-2 text-sm">{t(website + "CHANNEL_GREETING_MESSAGE.LABEL")}<textarea className={input} value={inbox.greeting_message ?? ""} onChange={e => update("greeting_message", e.target.value)} /></label>}{web && toggle("enable_email_collect", popup + "ENABLE_EMAIL_COLLECT_BOX", popup + "ENABLE_EMAIL_COLLECT_BOX_SUB_TEXT")}{toggle("allow_messages_after_resolved", popup + "ALLOW_MESSAGES_AFTER_RESOLVED", popup + "ALLOW_MESSAGES_AFTER_RESOLVED_SUB_TEXT")}{!web && toggle("lock_to_single_conversation", popup + "LOCK_TO_SINGLE_CONVERSATION", popup + "LOCK_TO_SINGLE_CONVERSATION_SUB_TEXT")}<div><Button type="submit" disabled={busy || !inbox.name.trim()} label={t(popup + "UPDATE")} /></div></form> : currentTab === "collaborators" ? <div className="max-w-2xl"><h2 className="font-medium">{t(popup + "INBOX_AGENTS")}</h2><p className="mb-3 text-sm text-n-slate-11">{t(popup + "INBOX_AGENTS_SUB_TEXT")}</p><div className="max-h-72 overflow-auto">{agents.map(a => <label key={a.id} className="flex items-center gap-3 border-b border-n-weak p-2"><input type="checkbox" checked={selected.includes(a.id)} onChange={e => setSelected(e.target.checked ? [...selected, a.id] : selected.filter(id => id !== a.id))} /><span>{a.name} <span className="text-sm text-n-slate-11">{a.email}</span></span></label>)}</div><div className="my-4"><Button disabled={busy} label={t(popup + "UPDATE")} onClick={saveAgents} /></div><h2 className="font-medium">{t(popup + "AGENT_ASSIGNMENT")}</h2><label className="mt-3 flex items-start gap-3 text-sm"><input type="checkbox" checked={!!inbox.enable_auto_assignment} disabled={busy} onChange={e => { void patch({ enable_auto_assignment: e.target.checked }); }} /><span>{t(popup + "AUTO_ASSIGNMENT")}<span className="mt-1 block text-n-slate-11">{t(popup + "AUTO_ASSIGNMENT_SUB_TEXT")}</span></span></label></div> : <div className="grid max-w-2xl gap-6">{web ? <><div><h2 className="font-medium">{t(popup + "MESSENGER_HEADING")}</h2><p className="text-sm text-n-slate-11">{t(popup + "MESSENGER_SUB_HEAD")}</p><textarea readOnly className={input + " mt-2 font-mono text-sm"} rows={4} value={inbox.web_widget_script ?? ""} /></div><div><h2 className="font-medium">{t(popup + "IDENTITY_VALIDATION.TITLE")}</h2><p className="text-sm text-n-slate-11">{t(popup + "IDENTITY_VALIDATION.DESCRIPTION")}</p><input readOnly type="password" className={input + " mt-2"} aria-label={t(popup + "IDENTITY_VALIDATION.SECRET_KEY")} value={inbox.hmac_token ?? ""} /></div></> : <><div><h2 className="font-medium">{t(popup + "INBOX_IDENTIFIER")}</h2><p className="text-sm text-n-slate-11">{t(popup + "INBOX_IDENTIFIER_SUB_TEXT")}</p><input readOnly className={input + " mt-2"} value={inbox.inbox_identifier ?? ""} aria-label={t(popup + "INBOX_IDENTIFIER")} /></div><div><h2 className="font-medium">{t(popup + "HMAC_VERIFICATION")}</h2><p className="text-sm text-n-slate-11">{t(popup + "HMAC_DESCRIPTION")}</p><input readOnly type="password" className={input + " mt-2"} value={inbox.hmac_token ?? ""} aria-label={t(popup + "HMAC_VERIFICATION")} /></div><label className="flex items-start gap-3 text-sm"><input type="checkbox" disabled={busy} checked={!!inbox.hmac_mandatory} onChange={e => { void patch({ channel: { hmac_mandatory: e.target.checked } }); }} /><span>{t(popup + "HMAC_MANDATORY_VERIFICATION")}<span className="block text-n-slate-11">{t(popup + "HMAC_MANDATORY_DESCRIPTION")}</span></span></label></>}</div>}
    </>}
    {notice && <p role="status" className="mt-4">{notice}</p>}{error && <p role="alert" className="mt-4 text-n-ruby-11">{error}</p>}
    {deleting && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"><div role="dialog" aria-modal="true" aria-labelledby="delete-inbox" className="w-full max-w-lg rounded-xl bg-n-background p-6"><h2 id="delete-inbox" className="text-lg font-medium">{t("INBOX_MGMT.DELETE.CONFIRM.TITLE")}</h2><p className="my-4">{t("INBOX_MGMT.DELETE.CONFIRM.MESSAGE")} {deleting.name}?</p><input className={input} value={confirm} onChange={e => setConfirm(e.target.value)} aria-label={t("INBOX_MGMT.DELETE.CONFIRM.PLACE_HOLDER", { inboxName: deleting.name })} placeholder={t("INBOX_MGMT.DELETE.CONFIRM.PLACE_HOLDER", { inboxName: deleting.name })} /><div className="mt-4 flex justify-end gap-2"><Button slate faded label={`${t("INBOX_MGMT.DELETE.CONFIRM.NO")} ${deleting.name}`} onClick={() => setDeleting(null)} /><Button ruby disabled={busy || confirm !== deleting.name} label={`${t("INBOX_MGMT.DELETE.CONFIRM.YES")} ${deleting.name}`} onClick={remove} /></div></div></div>}
  </section>;
}
