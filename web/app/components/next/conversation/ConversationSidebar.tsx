import { useEffect, useState } from "react";
import { acct, api, type Agent, type Conversation, type Label, type Team } from "~/lib/api";

export function ConversationSidebar({ accountId, conversation, onChange }: { accountId: number; conversation: Conversation; onChange: (conversation: Conversation) => void }) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [teams, setTeams] = useState<Team[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [linearIssues, setLinearIssues] = useState<Array<{ id: string; url?: string; issue?: { identifier: string; title: string; state?: { name: string } } }>>([]);
  const [shopifyOrders, setShopifyOrders] = useState<Array<{ id: number | string; name?: string; total_price?: string; financial_status?: string }>>([]);
  const [stripeData, setStripeData] = useState<{ customer?: { name?: string; email?: string }; subscriptions?: Array<{ id: string; status: string }>; invoices?: Array<{ id: string; status?: string; total?: number; currency?: string }> } | null>(null);
  const [linearQuery, setLinearQuery] = useState("");
  const [error, setError] = useState("");
  const base = `${acct(accountId)}/conversations/${conversation.id}`;
  const contact = conversation.meta.sender;
  useEffect(() => {
    let live = true;
    Promise.all([api<Agent[]>(`${acct(accountId)}/agents`), api<Team[]>(`${acct(accountId)}/teams`), api<{ payload: Label[] }>(`${acct(accountId)}/labels`)])
      .then(([a, t, l]) => { if (live) { setAgents(a); setTeams(t); setLabels(l.payload); } })
      .catch((e) => { if (live) setError(e instanceof Error ? e.message : "Could not load conversation details"); });
    api<Array<{ id: string; url?: string; issue?: { identifier: string; title: string; state?: { name: string } } }>>(
      `${acct(accountId)}/integrations/linear/linked_issues?conversation_id=${conversation.id}`
    ).then((res) => { if (live && Array.isArray(res)) setLinearIssues(res); }).catch(() => {});
    if (contact?.id) {
      api<{ orders: Array<{ id: number | string; name?: string; total_price?: string; financial_status?: string }> }>(
        `${acct(accountId)}/integrations/shopify/orders?contact_id=${contact.id}`
      ).then((res) => { if (live && Array.isArray(res.orders)) setShopifyOrders(res.orders); }).catch(() => {});
    }
    api<{ customer?: { name?: string; email?: string }; subscriptions?: Array<{ id: string; status: string }>; invoices?: Array<{ id: string; status?: string; total?: number; currency?: string }> }>(
      `${acct(accountId)}/integrations/stripe/customer?conversation_id=${conversation.id}`
    ).then((res) => { if (live && res.customer) setStripeData(res); }).catch(() => {});
    return () => { live = false; };
  }, [accountId, conversation.id, contact?.id]);
  async function update(path: string, json: object) {
    setError("");
    try {
      await api(`${base}/${path}`, { method: "POST", json });
      onChange(await api<Conversation>(base));
    } catch (e) { setError(e instanceof Error ? e.message : "Could not update conversation"); }
  }
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
    </div>
    {linearIssues.length > 0 && (
      <div className="mt-5 border-t border-line pt-4">
        <h3 className="mb-2 text-xs font-semibold uppercase text-ink-3">Linear Issues</h3>
        <div className="space-y-1.5 text-xs">
          {linearIssues.map((link) => (
            <div key={link.id} className="flex items-center justify-between rounded border border-line p-2">
              <span className="truncate font-medium">{link.issue?.identifier || link.id}: {link.issue?.title}</span>
              <button
                type="button"
                className="text-bad ml-2"
                onClick={async () => {
                  await api(`${acct(accountId)}/integrations/linear/unlink_issue`, {
                    method: "POST",
                    json: { conversation_id: conversation.id, link_id: link.id },
                  });
                  setLinearIssues((prev) => prev.filter((x) => x.id !== link.id));
                }}
              >
                Unlink
              </button>
            </div>
          ))}
        </div>
      </div>
    )}
    <div className="mt-4">
      <input
        value={linearQuery}
        onChange={(e) => setLinearQuery(e.target.value)}
        placeholder="Link Linear issue ID"
        onKeyDown={async (e) => {
          if (e.key === "Enter" && linearQuery.trim()) {
            try {
              const linked = await api<{ id: string; url?: string; issue?: { identifier: string; title: string } }>(
                `${acct(accountId)}/integrations/linear/link_issue`,
                { method: "POST", json: { conversation_id: conversation.id, issue_id: linearQuery.trim() } }
              );
              setLinearIssues((prev) => [...prev, linked]);
              setLinearQuery("");
            } catch {
              // ignore when linear is not connected
            }
          }
        }}
        className="w-full rounded-md border border-line bg-surface px-2 py-1 text-xs"
      />
    </div>
    {shopifyOrders.length > 0 && (
      <div className="mt-5 border-t border-line pt-4">
        <h3 className="mb-2 text-xs font-semibold uppercase text-ink-3">Shopify Orders</h3>
        <div className="space-y-1.5 text-xs">
          {shopifyOrders.map((o) => (
            <div key={String(o.id)} className="rounded border border-line p-2">
              <div className="font-medium">{o.name || `#${o.id}`}</div>
              <div className="text-ink-3">{o.financial_status} {o.total_price ? `· ${o.total_price}` : ""}</div>
            </div>
          ))}
        </div>
      </div>
    )}
    {stripeData?.customer && (
      <div className="mt-5 border-t border-line pt-4">
        <h3 className="mb-2 text-xs font-semibold uppercase text-ink-3">Stripe</h3>
        <div className="space-y-1.5 text-xs">
          <div className="font-medium">{stripeData.customer.name || stripeData.customer.email}</div>
          {(stripeData.subscriptions || []).map((s) => (
            <div key={s.id} className="rounded border border-line p-1.5">Sub {s.id}: {s.status}</div>
          ))}
          {(stripeData.invoices || []).map((inv) => (
            <div key={inv.id} className="text-ink-3">Inv {inv.id}: {inv.status}</div>
          ))}
        </div>
      </div>
    )}
    {error && <p role="alert" className="mt-3 text-sm text-bad">{error}</p>}
  </aside>;
}
