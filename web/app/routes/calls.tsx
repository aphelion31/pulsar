import { Phone, PhoneCall, PhoneIncoming, PhoneOutgoing, Plus, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import { Button, Empty } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "./shell";

type CallRecord = {
  id: number;
  inbox_id: number;
  conversation_id: number;
  conversation_display_id: number;
  contact_id: number;
  provider_call_id: string;
  provider: "twilio" | "whatsapp";
  direction: "inbound" | "outbound";
  status: string;
  duration_seconds: number | null;
  started_at: number | null;
  created_at: number;
  contact?: { id: number; name: string; phone_number?: string | null };
  inbox?: { id: number; name: string; channel_type: string };
  accepted_by?: { id: number; name: string } | null;
};

type InboxOption = {
  id: number;
  name: string;
  channel_type: string;
  voice_enabled?: boolean;
  provider_config?: { voice_enabled?: boolean };
};

type ContactOption = { id: number; name: string; phone_number?: string | null };

export default function CallsRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [calls, setCalls] = useState<CallRecord[]>([]);
  const [inboxes, setInboxes] = useState<InboxOption[]>([]);
  const [contacts, setContacts] = useState<ContactOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState("");
  const [directionFilter, setDirectionFilter] = useState("");
  const [inboxId, setInboxId] = useState("");
  const [contactId, setContactId] = useState("");
  const [toNumber, setToNumber] = useState("");
  const [dialBusy, setDialBusy] = useState(false);
  const [error, setError] = useState("");

  async function loadAll() {
    setLoading(true);
    setError("");
    try {
      const qs = new URLSearchParams();
      if (statusFilter) qs.set("status", statusFilter);
      if (directionFilter) qs.set("direction", directionFilter);
      const suffix = qs.toString() ? `?${qs.toString()}` : "";
      const [callsRes, inboxesRes, contactsRes] = await Promise.all([
        api<{ payload: CallRecord[] }>(`${acct(accountId)}/calls${suffix}`),
        api<{ payload: InboxOption[] }>(`${acct(accountId)}/inboxes`),
        api<{ payload: ContactOption[] }>(`${acct(accountId)}/contacts?page=1`),
      ]);
      setCalls(callsRes.payload ?? []);
      const voiceInboxes = (inboxesRes.payload ?? []).filter(
        (i) => i.voice_enabled || i.provider_config?.voice_enabled || i.channel_type === "Channel::TwilioSms",
      );
      setInboxes(voiceInboxes);
      if (!inboxId && voiceInboxes[0]) setInboxId(String(voiceInboxes[0].id));
      const cList = contactsRes.payload ?? [];
      setContacts(cList);
      if (!contactId && cList[0]) setContactId(String(cList[0].id));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load calls");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId, statusFilter, directionFilter]);

  async function startCall(e: React.FormEvent) {
    e.preventDefault();
    if (!contactId || !inboxId) return;
    setDialBusy(true);
    setError("");
    try {
      await api(`${acct(accountId)}/contacts/${contactId}/call`, {
        method: "POST",
        json: {
          inbox_id: Number(inboxId),
          to_number: toNumber.trim() || undefined,
        },
      });
      setToNumber("");
      await loadAll();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not initiate call");
    } finally {
      setDialBusy(false);
    }
  }

  async function waAction(callId: number, action: "accept" | "reject" | "terminate") {
    setError("");
    try {
      await api(`${acct(accountId)}/whatsapp_calls/${callId}/${action}`, { method: "POST" });
      await loadAll();
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to ${action} call`);
    }
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg p-6">
      <div className="mx-auto w-full max-w-6xl space-y-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold text-ink">Voice Calls</h1>
            <p className="text-sm text-ink-3">Twilio Voice & WhatsApp Calling call log and outbound dialer.</p>
          </div>
          <div className="flex items-center gap-2">
            <select
              aria-label="Filter direction"
              value={directionFilter}
              onChange={(e) => setDirectionFilter(e.target.value)}
              className="h-9 rounded-lg border border-line bg-surface px-3 text-sm text-ink"
            >
              <option value="">All directions</option>
              <option value="inbound">Inbound</option>
              <option value="outbound">Outbound</option>
            </select>
            <select
              aria-label="Filter status"
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
              className="h-9 rounded-lg border border-line bg-surface px-3 text-sm text-ink"
            >
              <option value="">All statuses</option>
              <option value="ringing">Ringing</option>
              <option value="in-progress">In progress</option>
              <option value="completed">Completed</option>
              <option value="rejected">Rejected</option>
            </select>
            <Button variant="quiet" onClick={() => void loadAll()}>
              <RefreshCw size={15} /> Refresh
            </Button>
          </div>
        </div>

        <form onSubmit={startCall} className="grid gap-3 rounded-xl border border-line bg-surface p-4 md:grid-cols-4">
          <label className="flex flex-col gap-1 text-xs font-medium text-ink-2">
            Voice Inbox
            <select
              value={inboxId}
              onChange={(e) => setInboxId(e.target.value)}
              className="h-9 rounded-lg border border-line bg-bg px-3 text-sm text-ink"
            >
              {inboxes.length === 0 && <option value="">No voice inbox</option>}
              {inboxes.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-ink-2">
            Contact
            <select
              value={contactId}
              onChange={(e) => setContactId(e.target.value)}
              className="h-9 rounded-lg border border-line bg-bg px-3 text-sm text-ink"
            >
              {contacts.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name} {c.phone_number ? `(${c.phone_number})` : ""}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-ink-2">
            Phone override (optional)
            <input
              type="tel"
              value={toNumber}
              onChange={(e) => setToNumber(e.target.value)}
              placeholder="+14155550100"
              className="h-9 rounded-lg border border-line bg-bg px-3 text-sm text-ink"
            />
          </label>
          <div className="flex items-end">
            <Button type="submit" disabled={dialBusy || !inboxId || !contactId} className="w-full">
              <PhoneCall size={15} /> {dialBusy ? "Dialing..." : "Start Outbound Call"}
            </Button>
          </div>
        </form>

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        {loading ? (
          <div className="rounded-xl border border-line bg-surface p-8 text-center text-sm text-ink-3">Loading calls...</div>
        ) : calls.length === 0 ? (
          <Empty title="No voice calls yet">
            <p className="text-sm text-ink-3">Configure a Twilio Voice or WhatsApp Calling inbox to place and receive calls.</p>
            <Link to={`/accounts/${accountId}/settings/inboxes/new`} className="mt-3 inline-flex items-center gap-1.5 text-sm font-medium text-accent hover:underline">
              <Plus size={14} /> Add Voice Inbox
            </Link>
          </Empty>
        ) : (
          <div className="overflow-x-auto rounded-xl border border-line bg-surface">
            <table className="w-full text-left text-sm">
              <thead className="border-b border-line bg-raised text-xs uppercase text-ink-3">
                <tr>
                  <th className="px-4 py-3">Contact</th>
                  <th className="px-4 py-3">Direction</th>
                  <th className="px-4 py-3">Provider</th>
                  <th className="px-4 py-3">Inbox</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Duration</th>
                  <th className="px-4 py-3 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {calls.map((c) => (
                  <tr key={c.id} className="hover:bg-raised/40">
                    <td className="px-4 py-3">
                      <div className="font-medium text-ink">{c.contact?.name || "Unknown"}</div>
                      <div className="text-xs text-ink-3">{c.contact?.phone_number || c.provider_call_id}</div>
                    </td>
                    <td className="px-4 py-3">
                      <span className="inline-flex items-center gap-1 capitalize text-ink-2">
                        {c.direction === "inbound" ? <PhoneIncoming size={14} /> : <PhoneOutgoing size={14} />}
                        {c.direction}
                      </span>
                    </td>
                    <td className="px-4 py-3 capitalize text-ink-2">
                      <span className="inline-flex items-center gap-1">
                        <Phone size={13} /> {c.provider}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-ink-2">{c.inbox?.name || `#${c.inbox_id}`}</td>
                    <td className="px-4 py-3">
                      <span className="rounded-full bg-raised px-2.5 py-0.5 text-xs font-medium capitalize text-ink">
                        {c.status}
                      </span>
                    </td>
                    <td className="px-4 py-3 font-mono text-xs text-ink-2">
                      {c.duration_seconds != null ? `${c.duration_seconds}s` : "—"}
                    </td>
                    <td className="px-4 py-3 text-right space-x-2">
                      {c.provider === "whatsapp" && c.status === "ringing" && (
                        <>
                          <button onClick={() => void waAction(c.id, "accept")} className="text-xs font-medium text-accent hover:underline">
                            Accept
                          </button>
                          <button onClick={() => void waAction(c.id, "reject")} className="text-xs font-medium text-bad hover:underline">
                            Reject
                          </button>
                        </>
                      )}
                      {c.provider === "whatsapp" && c.status === "in-progress" && (
                        <button onClick={() => void waAction(c.id, "terminate")} className="text-xs font-medium text-bad hover:underline">
                          End
                        </button>
                      )}
                      <Link
                        to={`/accounts/${accountId}/conversations/${c.conversation_display_id}`}
                        className="text-xs font-medium text-accent hover:underline"
                      >
                        Conversation #{c.conversation_display_id}
                      </Link>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
