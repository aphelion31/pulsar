// Chatwoot-style ContactPanel right sidebar with collapsible Accordion cards.
import {
  Check,
  Copy,
  ExternalLink,
  Mail,
  Minus,
  Phone,
  Play,
  Plus,
  Tag,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import Avatar from "~/components/next/Avatar";
import {
  acct,
  api,
  type Agent,
  type Conversation,
  type Label,
  type Team,
} from "~/lib/api";
import { timeAgo } from "~/lib/format";

type MacroItem = { id: number; name: string; visibility?: string };
type LinearIssue = {
  id: string;
  url?: string;
  issue?: { identifier: string; title: string; state?: { name: string } };
};
type ShopifyOrder = {
  id: number | string;
  name?: string;
  total_price?: string;
  financial_status?: string;
};
type StripeData = {
  customer?: { name?: string; email?: string };
  subscriptions?: Array<{ id: string; status: string }>;
  invoices?: Array<{ id: string; status?: string }>;
};

function AccordionItem({
  title,
  defaultOpen = true,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="rounded-lg border border-n-weak bg-n-solid-1">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={`flex w-full items-center justify-between px-3.5 py-2.5 text-left text-xs font-medium text-n-slate-12 transition-colors ${
          open ? "rounded-t-lg border-b border-n-weak" : "rounded-lg"
        } bg-n-slate-2 dark:bg-n-solid-2 hover:bg-n-alpha-1`}
      >
        <span>{title}</span>
        {open ? (
          <Minus className="size-3.5 text-n-blue-11" />
        ) : (
          <Plus className="size-3.5 text-n-blue-11" />
        )}
      </button>
      {open && <div className="p-3">{children}</div>}
    </div>
  );
}

export function ConversationSidebar({
  accountId,
  conversation,
  inboxName,
  onChange,
  onClose,
}: {
  accountId: number;
  conversation: Conversation;
  inboxName?: string;
  onChange: (conversation: Conversation) => void;
  onClose?: () => void;
}) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [teams, setTeams] = useState<Team[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [macros, setMacros] = useState<MacroItem[]>([]);
  const [prevConvs, setPrevConvs] = useState<Conversation[]>([]);
  const [linearIssues, setLinearIssues] = useState<LinearIssue[]>([]);
  const [shopifyOrders, setShopifyOrders] = useState<ShopifyOrder[]>([]);
  const [stripeData, setStripeData] = useState<StripeData | null>(null);
  const [enabledIntegrations, setEnabledIntegrations] = useState<Set<string>>(new Set());
  const [linearQuery, setLinearQuery] = useState("");
  const [copied, setCopied] = useState<string>("");
  const [macroBusy, setMacroBusy] = useState<number | null>(null);
  const [error, setError] = useState("");

  const base = `${acct(accountId)}/conversations/${conversation.id}`;
  const contact = conversation.meta.sender;

  useEffect(() => {
    let live = true;
    Promise.all([
      api<Agent[]>(`${acct(accountId)}/agents`),
      api<Team[]>(`${acct(accountId)}/teams`),
      api<{ payload: Label[] }>(`${acct(accountId)}/labels`),
    ])
      .then(([a, t, l]) => {
        if (!live) return;
        setAgents(Array.isArray(a) ? a : []);
        setTeams(Array.isArray(t) ? t : []);
        setLabels(l?.payload ?? []);
      })
      .catch((e) => {
        if (live) setError(e instanceof Error ? e.message : "Failed to load conversation details");
      });

    api<MacroItem[] | { payload: MacroItem[] }>(`${acct(accountId)}/macros`)
      .then((res) => {
        if (live) setMacros(Array.isArray(res) ? res : res?.payload ?? []);
      })
      .catch(() => {});

    api<{ payload: Array<{ id: string; enabled?: boolean }> }>(`${acct(accountId)}/integrations/apps`)
      .then((res) => {
        if (!live) return;
        const enabled = new Set(
          (res?.payload ?? []).filter((app) => app.enabled).map((app) => app.id)
        );
        setEnabledIntegrations(enabled);

        if (enabled.has("linear")) {
          api<LinearIssue[]>(
            `${acct(accountId)}/integrations/linear/linked_issues?conversation_id=${conversation.id}`
          )
            .then((items) => {
              if (live && Array.isArray(items)) setLinearIssues(items);
            })
            .catch(() => {});
        }
        if (enabled.has("shopify") && contact?.id) {
          api<{ orders: ShopifyOrder[] }>(
            `${acct(accountId)}/integrations/shopify/orders?contact_id=${contact.id}`
          )
            .then((ordersRes) => {
              if (live && Array.isArray(ordersRes?.orders)) setShopifyOrders(ordersRes.orders);
            })
            .catch(() => {});
        }
        if (enabled.has("stripe")) {
          api<StripeData>(
            `${acct(accountId)}/integrations/stripe/customer?conversation_id=${conversation.id}`
          )
            .then((stripeRes) => {
              if (live && stripeRes?.customer) setStripeData(stripeRes);
            })
            .catch(() => {});
        }
      })
      .catch(() => {});

    if (contact?.id) {
      api<{ payload: Conversation[] } | Conversation[]>(
        `${acct(accountId)}/contacts/${contact.id}/conversations`
      )
        .then((res) => {
          if (!live) return;
          const items = Array.isArray(res) ? res : res?.payload ?? [];
          setPrevConvs(items.filter((c) => c.id !== conversation.id).slice(0, 5));
        })
        .catch(() => {});
    }

    return () => {
      live = false;
    };
  }, [accountId, conversation.id, contact?.id]);

  async function update(path: string, json: object) {
    setError("");
    try {
      await api(`${base}/${path}`, { method: "POST", json });
      onChange(await api<Conversation>(base));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not update conversation");
    }
  }

  async function runMacro(m: MacroItem) {
    setMacroBusy(m.id);
    setError("");
    try {
      await api(`${acct(accountId)}/macros/${m.id}/execute`, {
        method: "POST",
        json: { conversation_ids: [conversation.id] },
      });
      onChange(await api<Conversation>(base));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not execute macro");
    } finally {
      setMacroBusy(null);
    }
  }

  function copyVal(key: string, val: string) {
    navigator.clipboard?.writeText(val).catch(() => {});
    setCopied(key);
    setTimeout(() => setCopied(""), 1500);
  }

  const selectCls =
    "mt-1 block h-8 w-full rounded-lg border border-n-weak bg-n-background px-2.5 text-xs text-n-slate-12 focus:border-n-brand focus:outline-none";

  const customAttrs = Object.entries(contact.custom_attributes ?? {});
  const convExtra = Object.entries(conversation.additional_attributes ?? {}).filter(
    ([, v]) => typeof v === "string" || typeof v === "number"
  );

  return (
    <aside
      aria-label="Conversation details"
      className="flex h-full w-80 shrink-0 flex-col overflow-y-auto border-l border-n-weak bg-n-surface-1 max-xl:hidden"
    >
      {/* SidebarActionsHeader */}
      <div className="flex h-12 shrink-0 items-center justify-between border-b border-n-weak px-4">
        <h2 className="text-sm font-semibold text-n-slate-12">Contact</h2>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            aria-label="Close contact panel"
            className="grid size-7 place-items-center rounded-lg text-n-slate-10 hover:bg-n-alpha-2 hover:text-n-slate-12"
          >
            <X className="size-4" />
          </button>
        )}
      </div>

      {/* Contact Info Header Card */}
      <div className="border-b border-n-weak p-4">
        <div className="flex items-start gap-3">
          <Avatar
            size={48}
            name={contact.name || "Unknown"}
            src={contact.thumbnail}
            status={contact.availability_status === "online" ? "online" : undefined}
          />
          <div className="min-w-0 flex-1">
            <Link
              to={`/accounts/${accountId}/contacts/${contact.id}`}
              className="group inline-flex items-center gap-1 text-sm font-semibold text-n-slate-12 hover:text-n-brand"
            >
              <span className="truncate">{contact.name || "Unknown contact"}</span>
              <ExternalLink className="size-3 shrink-0 opacity-0 transition group-hover:opacity-100" />
            </Link>
            {contact.email && (
              <div className="mt-1 flex items-center gap-1.5 text-xs text-n-slate-11">
                <Mail className="size-3.5 shrink-0 text-n-slate-10" />
                <span className="truncate">{contact.email}</span>
                <button
                  type="button"
                  onClick={() => copyVal("email", contact.email!)}
                  title="Copy email"
                  className="ml-auto shrink-0 text-n-slate-10 hover:text-n-slate-12"
                >
                  {copied === "email" ? <Check className="size-3 text-n-teal-11" /> : <Copy className="size-3" />}
                </button>
              </div>
            )}
            {contact.phone_number && (
              <div className="mt-1 flex items-center gap-1.5 text-xs text-n-slate-11">
                <Phone className="size-3.5 shrink-0 text-n-slate-10" />
                <span className="truncate">{contact.phone_number}</span>
                <button
                  type="button"
                  onClick={() => copyVal("phone", contact.phone_number!)}
                  title="Copy phone"
                  className="ml-auto shrink-0 text-n-slate-10 hover:text-n-slate-12"
                >
                  {copied === "phone" ? <Check className="size-3 text-n-teal-11" /> : <Copy className="size-3" />}
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Accordion Stack */}
      <div className="flex flex-col gap-3 p-3">
        <AccordionItem title="Conversation Actions" defaultOpen>
          <div className="space-y-3">
            <label className="block text-xs font-medium text-n-slate-11">
              Assigned Agent
              <select
                className={selectCls}
                value={conversation.meta.assignee?.id ?? ""}
                onChange={(e) =>
                  update("assignments", {
                    assignee_id: e.target.value ? Number(e.target.value) : null,
                  })
                }
              >
                <option value="">Unassigned</option>
                {agents.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.available_name || a.name}
                  </option>
                ))}
              </select>
            </label>

            <label className="block text-xs font-medium text-n-slate-11">
              Assigned Team
              <select
                className={selectCls}
                value={conversation.meta.team?.id ?? ""}
                onChange={(e) =>
                  update("assignments", {
                    team_id: e.target.value ? Number(e.target.value) : null,
                  })
                }
              >
                <option value="">None</option>
                {teams.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </select>
            </label>

            <label className="block text-xs font-medium text-n-slate-11">
              Priority
              <select
                className={selectCls}
                value={conversation.priority ?? ""}
                onChange={(e) =>
                  update("toggle_priority", { priority: e.target.value || null })
                }
              >
                <option value="">None</option>
                <option value="low">Low</option>
                <option value="medium">Medium</option>
                <option value="high">High</option>
                <option value="urgent">Urgent</option>
              </select>
            </label>

            <div>
              <div className="mb-1.5 flex items-center gap-1 text-xs font-medium text-n-slate-11">
                <Tag className="size-3" /> Conversation Labels
              </div>
              {labels.length === 0 ? (
                <p className="text-xs text-n-slate-10">No labels configured</p>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {labels.map((l) => {
                    const active = conversation.labels.includes(l.title);
                    return (
                      <button
                        key={l.id}
                        type="button"
                        onClick={() =>
                          update("labels", {
                            labels: active
                              ? conversation.labels.filter((x) => x !== l.title)
                              : [...conversation.labels, l.title],
                          })
                        }
                        className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-xs transition ${
                          active
                            ? "border-n-blue-7 bg-n-blue-3/60 font-medium text-n-blue-11 dark:bg-n-solid-blue dark:text-n-blue-text"
                            : "border-n-weak bg-n-background text-n-slate-11 hover:bg-n-alpha-1"
                        }`}
                      >
                        <span
                          className="size-2 rounded-full shrink-0"
                          style={{ backgroundColor: l.color || "#64748b" }}
                        />
                        {l.title}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        </AccordionItem>

        {macros.length > 0 && (
          <AccordionItem title="Macros" defaultOpen={false}>
            <div className="space-y-1.5">
              {macros.map((m) => (
                <div
                  key={m.id}
                  className="flex items-center justify-between rounded-lg border border-n-weak bg-n-background px-2.5 py-1.5 text-xs"
                >
                  <span className="truncate font-medium text-n-slate-12">{m.name}</span>
                  <button
                    type="button"
                    disabled={macroBusy === m.id}
                    onClick={() => void runMacro(m)}
                    title="Execute macro"
                    className="grid size-6 shrink-0 place-items-center rounded text-n-blue-11 hover:bg-n-alpha-2 disabled:opacity-40"
                  >
                    <Play className="size-3 fill-current" />
                  </button>
                </div>
              ))}
            </div>
          </AccordionItem>
        )}

        <AccordionItem title="Conversation Information" defaultOpen={false}>
          <dl className="space-y-2 text-xs">
            <div className="flex justify-between gap-2">
              <dt className="text-n-slate-10">Conversation ID</dt>
              <dd className="font-mono text-n-slate-12">#{conversation.id}</dd>
            </div>
            {inboxName && (
              <div className="flex justify-between gap-2">
                <dt className="text-n-slate-10">Channel</dt>
                <dd className="truncate text-n-slate-12">{inboxName}</dd>
              </div>
            )}
            <div className="flex justify-between gap-2">
              <dt className="text-n-slate-10">Created</dt>
              <dd className="text-n-slate-12">
                {conversation.created_at ? timeAgo(conversation.created_at) : timeAgo(conversation.timestamp)}
              </dd>
            </div>
            {convExtra.map(([k, v]) => (
              <div key={k} className="flex justify-between gap-2">
                <dt className="capitalize text-n-slate-10">{k.replaceAll("_", " ")}</dt>
                <dd className="max-w-36 truncate text-n-slate-12" title={String(v)}>
                  {String(v)}
                </dd>
              </div>
            ))}
          </dl>
        </AccordionItem>

        {customAttrs.length > 0 && (
          <AccordionItem title="Contact Attributes" defaultOpen={false}>
            <dl className="space-y-1.5 text-xs">
              {customAttrs.map(([k, v]) => (
                <div key={k} className="flex justify-between gap-2 rounded bg-n-background px-2 py-1">
                  <dt className="text-n-slate-10">{k}</dt>
                  <dd className="truncate font-medium text-n-slate-12">{String(v)}</dd>
                </div>
              ))}
            </dl>
          </AccordionItem>
        )}

        {prevConvs.length > 0 && (
          <AccordionItem title="Previous Conversations" defaultOpen={false}>
            <div className="space-y-1.5">
              {prevConvs.map((c) => (
                <Link
                  key={c.id}
                  to={`/accounts/${accountId}/conversations/${c.id}`}
                  className="flex items-center justify-between rounded-lg border border-n-weak bg-n-background px-2.5 py-2 text-xs hover:bg-n-alpha-1"
                >
                  <span className="font-mono text-n-slate-12">#{c.id}</span>
                  <span className="capitalize text-n-slate-11">{c.status}</span>
                </Link>
              ))}
            </div>
          </AccordionItem>
        )}

        {(enabledIntegrations.has("linear") || linearIssues.length > 0) && (
          <AccordionItem title="Linear" defaultOpen={linearIssues.length > 0}>
            {linearIssues.length > 0 && (
              <div className="mb-2 space-y-1.5 text-xs">
                {linearIssues.map((link) => (
                  <div
                    key={link.id}
                    className="flex items-center justify-between rounded-lg border border-n-weak bg-n-background p-2"
                  >
                    <span className="truncate font-medium text-n-slate-12">
                      {link.issue?.identifier || link.id}: {link.issue?.title}
                    </span>
                    <button
                      type="button"
                      className="ml-2 text-n-ruby-11 hover:underline"
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
            )}
            <input
              value={linearQuery}
              onChange={(e) => setLinearQuery(e.target.value)}
              placeholder="Link Linear issue ID (Enter)"
              onKeyDown={async (e) => {
                if (e.key === "Enter" && linearQuery.trim()) {
                  try {
                    const linked = await api<LinearIssue>(
                      `${acct(accountId)}/integrations/linear/link_issue`,
                      {
                        method: "POST",
                        json: { conversation_id: conversation.id, issue_id: linearQuery.trim() },
                      }
                    );
                    setLinearIssues((prev) => [...prev, linked]);
                    setLinearQuery("");
                  } catch {
                    // ignore when Linear is not connected
                  }
                }
              }}
              className="h-8 w-full rounded-lg border border-n-weak bg-n-background px-2.5 text-xs text-n-slate-12 placeholder:text-n-slate-10 focus:border-n-brand focus:outline-none"
            />
          </AccordionItem>
        )}

        {shopifyOrders.length > 0 && (
          <AccordionItem title="Shopify Orders" defaultOpen>
            <div className="space-y-1.5 text-xs">
              {shopifyOrders.map((o) => (
                <div key={String(o.id)} className="rounded-lg border border-n-weak bg-n-background p-2">
                  <div className="font-medium text-n-slate-12">{o.name || `#${o.id}`}</div>
                  <div className="text-n-slate-11">
                    {o.financial_status} {o.total_price ? `· ${o.total_price}` : ""}
                  </div>
                </div>
              ))}
            </div>
          </AccordionItem>
        )}

        {stripeData?.customer && (
          <AccordionItem title="Stripe" defaultOpen>
            <div className="space-y-1.5 text-xs">
              <div className="font-medium text-n-slate-12">
                {stripeData.customer.name || stripeData.customer.email}
              </div>
              {(stripeData.subscriptions || []).map((s) => (
                <div key={s.id} className="rounded-lg border border-n-weak bg-n-background p-2 text-n-slate-12">
                  Sub {s.id}: {s.status}
                </div>
              ))}
              {(stripeData.invoices || []).map((inv) => (
                <div key={inv.id} className="text-n-slate-11">
                  Inv {inv.id}: {inv.status}
                </div>
              ))}
            </div>
          </AccordionItem>
        )}

        {error && (
          <p role="alert" className="px-1 text-xs text-n-ruby-11">
            {error}
          </p>
        )}
      </div>
    </aside>
  );
}
