import { ArrowLeft, Plus, Send, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import { Button } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";
import { CaptainNav } from "./nav";

type AssistantDetail = {
  id: number;
  name: string;
  description: string | null;
  config: Record<string, unknown>;
  response_guidelines: string[];
  guardrails: string[];
};

type OverviewStats = {
  total_conversations: number;
  resolved_conversations: number;
  handoff_conversations: number;
  resolution_rate: number;
  faq_count: number;
  documents_count: number;
};

type Scenario = {
  id: number;
  title: string;
  description: string | null;
  instruction: string | null;
  enabled: boolean;
};

type InboxItem = { id: number; name: string; channel_type: string };

export default function CaptainAssistantDetailRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { assistantId, tab = "overview" } = useParams();
  const [assistant, setAssistant] = useState<AssistantDetail | null>(null);
  const [stats, setStats] = useState<OverviewStats | null>(null);
  const [scenarios, setScenarios] = useState<Scenario[]>([]);
  const [connectedInboxes, setConnectedInboxes] = useState<InboxItem[]>([]);
  const [allInboxes, setAllInboxes] = useState<InboxItem[]>([]);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [chatHistory, setChatHistory] = useState<{ role: "user" | "assistant"; content: string }[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatBusy, setChatBusy] = useState(false);
  const [scTitle, setScTitle] = useState("");
  const [scInstruction, setScInstruction] = useState("");
  const [selectedInboxId, setSelectedInboxId] = useState("");
  const [error, setError] = useState("");

  async function load() {
    if (!assistantId) return;
    setError("");
    try {
      const [a, ov, sc, ci, ai] = await Promise.all([
        api<AssistantDetail>(`${acct(accountId)}/captain/assistants/${assistantId}`),
        api<OverviewStats>(`${acct(accountId)}/captain/assistants/${assistantId}/stats/overview`),
        api<{ payload: Scenario[] }>(`${acct(accountId)}/captain/assistants/${assistantId}/scenarios`),
        api<{ payload: InboxItem[] }>(`${acct(accountId)}/captain/assistants/${assistantId}/inboxes`),
        api<{ payload: InboxItem[] }>(`${acct(accountId)}/inboxes`),
      ]);
      setAssistant(a);
      setName(a.name);
      setDescription(a.description || "");
      setStats(ov);
      setScenarios(sc.payload ?? []);
      setConnectedInboxes(ci.payload ?? []);
      setAllInboxes(ai.payload ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load assistant");
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId, assistantId]);

  async function saveOverview(e: React.FormEvent) {
    e.preventDefault();
    if (!assistantId) return;
    setError("");
    try {
      const updated = await api<AssistantDetail>(`${acct(accountId)}/captain/assistants/${assistantId}`, {
        method: "PATCH",
        json: { assistant: { name: name.trim(), description: description.trim() || null } },
      });
      setAssistant(updated);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update assistant");
    }
  }

  async function sendPlayground(e: React.FormEvent) {
    e.preventDefault();
    if (!assistantId || !chatInput.trim()) return;
    const msg = chatInput.trim();
    setChatInput("");
    const nextHist = [...chatHistory, { role: "user" as const, content: msg }];
    setChatHistory(nextHist);
    setChatBusy(true);
    setError("");
    try {
      const res = await api<{ message?: string }>(`${acct(accountId)}/captain/assistants/${assistantId}/playground`, {
        method: "POST",
        json: { message_content: msg, message_history: chatHistory },
      });
      setChatHistory([...nextHist, { role: "assistant", content: res.message || "(Queued response)" }]);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Playground request failed");
    } finally {
      setChatBusy(false);
    }
  }

  async function createScenario(e: React.FormEvent) {
    e.preventDefault();
    if (!assistantId || !scTitle.trim()) return;
    setError("");
    try {
      await api(`${acct(accountId)}/captain/assistants/${assistantId}/scenarios`, {
        method: "POST",
        json: { scenario: { title: scTitle.trim(), instruction: scInstruction.trim() || null, enabled: true } },
      });
      setScTitle("");
      setScInstruction("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create scenario");
    }
  }

  async function deleteScenario(id: number) {
    if (!assistantId) return;
    await api(`${acct(accountId)}/captain/assistants/${assistantId}/scenarios/${id}`, { method: "DELETE" });
    await load();
  }

  async function connectInbox(e: React.FormEvent) {
    e.preventDefault();
    if (!assistantId || !selectedInboxId) return;
    await api(`${acct(accountId)}/captain/assistants/${assistantId}/inboxes`, {
      method: "POST",
      json: { inbox_id: Number(selectedInboxId) },
    });
    setSelectedInboxId("");
    await load();
  }

  async function disconnectInbox(inboxId: number) {
    if (!assistantId) return;
    await api(`${acct(accountId)}/captain/assistants/${assistantId}/inboxes/${inboxId}`, { method: "DELETE" });
    await load();
  }

  const subTabs = ["overview", "playground", "scenarios", "inboxes"];

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg">
      <CaptainNav accountId={accountId} active="assistants" />
      <div className="mx-auto w-full max-w-6xl space-y-6 p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <Link to={`/accounts/${accountId}/captain/assistants`} className="rounded-lg border border-line bg-surface p-2 text-ink-2 hover:text-ink">
              <ArrowLeft size={16} />
            </Link>
            <div>
              <h2 className="text-lg font-semibold text-ink">{assistant?.name || `Assistant #${assistantId}`}</h2>
              <div className="mt-1 flex gap-2 text-xs">
                {subTabs.map((st) => (
                  <Link
                    key={st}
                    to={`/accounts/${accountId}/captain/${assistantId}/${st}`}
                    className={`rounded-md px-2.5 py-1 capitalize ${
                      tab === st ? "bg-raised font-medium text-ink" : "text-ink-3 hover:text-ink"
                    }`}
                  >
                    {st}
                  </Link>
                ))}
              </div>
            </div>
          </div>
          <Link to={`/accounts/${accountId}/captain/${assistantId}/tools`} className="text-sm font-medium text-accent hover:underline">
            Configure Tools →
          </Link>
        </div>

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        {tab === "overview" && (
          <div className="space-y-6">
            {stats && (
              <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                {[
                  { label: "Conversations", val: stats.total_conversations },
                  { label: "Resolved", val: stats.resolved_conversations },
                  { label: "Resolution Rate", val: `${stats.resolution_rate.toFixed(1)}%` },
                  { label: "FAQs / Docs", val: `${stats.faq_count} / ${stats.documents_count}` },
                ].map((card) => (
                  <div key={card.label} className="rounded-xl border border-line bg-surface p-4">
                    <div className="text-xs text-ink-3">{card.label}</div>
                    <div className="mt-1 font-mono text-2xl font-semibold text-ink">{card.val}</div>
                  </div>
                ))}
              </div>
            )}
            <form onSubmit={saveOverview} className="space-y-4 rounded-xl border border-line bg-surface p-5">
              <label className="flex flex-col gap-1 text-sm font-medium text-ink">
                Name
                <input value={name} onChange={(e) => setName(e.target.value)} className="h-10 rounded-lg border border-line bg-bg px-3 text-sm" />
              </label>
              <label className="flex flex-col gap-1 text-sm font-medium text-ink">
                Description
                <textarea rows={3} value={description} onChange={(e) => setDescription(e.target.value)} className="rounded-lg border border-line bg-bg p-3 text-sm" />
              </label>
              <div className="flex justify-end">
                <Button type="submit">Save Settings</Button>
              </div>
            </form>
          </div>
        )}

        {tab === "playground" && (
          <div className="flex flex-col rounded-xl border border-line bg-surface">
            <div className="max-h-[420px] min-h-[280px] space-y-3 overflow-y-auto p-5">
              {chatHistory.length === 0 ? (
                <p className="text-sm text-ink-3">Send a test customer message to preview how this assistant responds.</p>
              ) : (
                chatHistory.map((m, idx) => (
                  <div key={idx} className={`flex ${m.role === "user" ? "justify-end" : "justify-start"}`}>
                    <div className={`max-w-xl rounded-xl px-4 py-2.5 text-sm ${m.role === "user" ? "bg-accent text-accent-ink" : "bg-raised text-ink"}`}>
                      {m.content}
                    </div>
                  </div>
                ))
              )}
            </div>
            <form onSubmit={sendPlayground} className="flex gap-2 border-t border-line p-4">
              <input
                value={chatInput}
                onChange={(e) => setChatInput(e.target.value)}
                placeholder="Ask the assistant a question..."
                className="h-10 flex-1 rounded-lg border border-line bg-bg px-3 text-sm text-ink"
              />
              <Button type="submit" disabled={chatBusy || !chatInput.trim()}>
                <Send size={15} /> {chatBusy ? "Thinking..." : "Send"}
              </Button>
            </form>
          </div>
        )}

        {tab === "scenarios" && (
          <div className="space-y-4">
            <form onSubmit={createScenario} className="space-y-3 rounded-xl border border-line bg-surface p-5">
              <h3 className="text-sm font-semibold text-ink">Add Scenario</h3>
              <input
                required
                value={scTitle}
                onChange={(e) => setScTitle(e.target.value)}
                placeholder="Scenario title (e.g. Refund Escalation)"
                className="h-10 w-full rounded-lg border border-line bg-bg px-3 text-sm"
              />
              <textarea
                rows={2}
                value={scInstruction}
                onChange={(e) => setScInstruction(e.target.value)}
                placeholder="Instructions for when this scenario triggers..."
                className="w-full rounded-lg border border-line bg-bg p-3 text-sm"
              />
              <div className="flex justify-end">
                <Button type="submit"><Plus size={14} /> Add Scenario</Button>
              </div>
            </form>
            <div className="space-y-2">
              {scenarios.map((s) => (
                <div key={s.id} className="flex items-start justify-between rounded-xl border border-line bg-surface p-4">
                  <div>
                    <div className="font-medium text-ink">{s.title}</div>
                    {s.instruction && <p className="mt-1 text-sm text-ink-2">{s.instruction}</p>}
                  </div>
                  <button onClick={() => void deleteScenario(s.id)} className="text-ink-3 hover:text-bad">
                    <Trash2 size={15} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        {tab === "inboxes" && (
          <div className="space-y-4">
            <form onSubmit={connectInbox} className="flex gap-2 rounded-xl border border-line bg-surface p-4">
              <select
                value={selectedInboxId}
                onChange={(e) => setSelectedInboxId(e.target.value)}
                className="h-10 flex-1 rounded-lg border border-line bg-bg px-3 text-sm"
              >
                <option value="">Select an inbox to connect...</option>
                {allInboxes
                  .filter((i) => !connectedInboxes.some((ci) => ci.id === i.id))
                  .map((i) => (
                    <option key={i.id} value={i.id}>{i.name} ({i.channel_type})</option>
                  ))}
              </select>
              <Button type="submit" disabled={!selectedInboxId}>Connect Inbox</Button>
            </form>
            <div className="space-y-2">
              {connectedInboxes.map((i) => (
                <div key={i.id} className="flex items-center justify-between rounded-xl border border-line bg-surface p-4">
                  <div>
                    <div className="font-medium text-ink">{i.name}</div>
                    <div className="text-xs text-ink-3">{i.channel_type}</div>
                  </div>
                  <Button variant="quiet" onClick={() => void disconnectInbox(i.id)}>Disconnect</Button>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
