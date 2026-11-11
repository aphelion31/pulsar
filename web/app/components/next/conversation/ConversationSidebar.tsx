import { useEffect, useState } from "react";
import { acct, api, type Agent, type Conversation, type Label, type Team } from "~/lib/api";

export function ConversationSidebar({ accountId, conversation, onChange }: { accountId: number; conversation: Conversation; onChange: (conversation: Conversation) => void }) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [teams, setTeams] = useState<Team[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [error, setError] = useState("");
  const base = `${acct(accountId)}/conversations/${conversation.id}`;
  useEffect(() => {
    let live = true;
    Promise.all([api<Agent[]>(`${acct(accountId)}/agents`), api<Team[]>(`${acct(accountId)}/teams`), api<Label[]>(`${acct(accountId)}/labels`)])
      .then(([a, t, l]) => { if (live) { setAgents(a); setTeams(t); setLabels(l); } })
      .catch((e) => { if (live) setError(e instanceof Error ? e.message : "Could not load conversation details"); });
    return () => { live = false; };
  }, [accountId]);
  async function update(path: string, json: object) {
    setError("");
    try {
      await api(`${base}/${path}`, { method: "POST", json });
      onChange(await api<Conversation>(base));
    } catch (e) { setError(e instanceof Error ? e.message : "Could not update conversation"); }
  }
  const contact = conversation.meta.sender;
  const select = "w-full rounded-md border border-line bg-surface px-2 py-1.5 text-sm";
  return <aside aria-label="Conversation details" className="w-72 shrink-0 overflow-y-auto border-l border-line bg-surface p-4 max-xl:hidden">
    <h2 className="mb-3 text-sm font-semibold">Contact details</h2>
    <div className="mb-5 space-y-1 text-sm"><p>{contact.name || "Unknown contact"}</p>{contact.email && <p className="break-all text-ink-3">{contact.email}</p>}{contact.phone_number && <p className="text-ink-3">{contact.phone_number}</p>}</div>
    <h2 className="mb-3 text-sm font-semibold">Conversation actions</h2>
    <div className="space-y-3">
      <label className="block text-xs text-ink-3">Assignee<select className={select} value={conversation.meta.assignee?.id ?? ""} onChange={e => update("assignments", { assignee_id: e.target.value ? Number(e.target.value) : null })}><option value="">Unassigned</option>{agents.map(a => <option key={a.id} value={a.id}>{a.available_name || a.name}</option>)}</select></label>
      <label className="block text-xs text-ink-3">Team<select className={select} value={conversation.meta.team?.id ?? ""} onChange={e => update("assignments", { team_id: e.target.value ? Number(e.target.value) : null })}><option value="">None</option>{teams.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}</select></label>
      <label className="block text-xs text-ink-3">Priority<select className={select} value={conversation.priority ?? ""} onChange={e => update("toggle_priority", { priority: e.target.value || null })}><option value="">None</option><option value="low">Low</option><option value="medium">Medium</option><option value="high">High</option><option value="urgent">Urgent</option></select></label>
      <fieldset><legend className="mb-1 text-xs text-ink-3">Labels</legend>{labels.map(l => <label key={l.id} className="flex items-center gap-2 py-1 text-sm"><input type="checkbox" checked={conversation.labels.includes(l.title)} onChange={e => update("labels", { labels: e.target.checked ? [...conversation.labels, l.title] : conversation.labels.filter(x => x !== l.title) })} />{l.title}</label>)}</fieldset>
    </div>{error && <p role="alert" className="mt-3 text-sm text-bad">{error}</p>}
  </aside>;
}
