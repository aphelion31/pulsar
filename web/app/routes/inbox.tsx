// Inbox: 3-pane Chatwoot conversation view (list, thread + ReplyBox, collapsible ContactPanel).
import {
  AlertCircle,
  BellOff,
  Check,
  CheckCircle2,
  ChevronDown,
  Clock,
  Copy,
  Globe,
  Lock,
  Mail,
  MessageSquare,
  MoreVertical,
  PanelRight,
  Paperclip,
  Phone,
  Reply,
  RotateCcw,
  Send,
  Trash2,
  UserCheck,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useOutletContext, useParams } from "react-router";
import Avatar from "~/components/next/Avatar";
import { ConversationSidebar } from "~/components/next/conversation/ConversationSidebar";
import { ReplyBox } from "~/components/next/conversation/ReplyBox";
import { Empty } from "~/components/ui";
import {
  acct,
  api,
  type Conversation,
  type Inbox,
  type Label,
  type Message,
} from "~/lib/api";
import { bytes, clock, timeAgo } from "~/lib/format";
import type { ShellCtx } from "./shell";

const TABS = [
  { key: "me", label: "Mine" },
  { key: "unassigned", label: "Unassigned" },
  { key: "all", label: "All" },
] as const;
type Tab = (typeof TABS)[number]["key"];
type Meta = { mine_count: number; unassigned_count: number; all_count: number };

function channelBadgeIcon(channelType?: string) {
  const t = (channelType || "").toLowerCase();
  if (t.includes("web")) return <Globe className="size-3 shrink-0" />;
  if (t.includes("email")) return <Mail className="size-3 shrink-0" />;
  if (t.includes("whatsapp") || t.includes("twilio") || t.includes("sms") || t.includes("voice"))
    return <Phone className="size-3 shrink-0" />;
  return <MessageSquare className="size-3 shrink-0" />;
}

function dayDividerLabel(unixSecs: number): string {
  const d = new Date(unixSecs * 1000);
  const now = new Date();
  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((startOfDay(now) - startOfDay(d)) / 86400000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

export default function InboxRoute() {
  const { me, accountId, subscribe } = useOutletContext<ShellCtx>();
  const params = useParams();
  const conversationId = params.conversationId ?? params.conversation_id;
  const kind = useLocation().pathname.split("/")[3];
  const scope = params.inbox_id
    ? `&inbox_id=${params.inbox_id}`
    : params.label
      ? `&labels[]=${encodeURIComponent(params.label)}`
      : params.teamId
        ? `&team_id=${params.teamId}`
        : kind === "mentions"
          ? "&conversation_type=mention"
          : kind === "unattended" || kind === "participating"
            ? `&conversation_type=${kind}`
            : "";
  const base = params.inbox_id
    ? `/accounts/${accountId}/inbox/${params.inbox_id}`
    : params.label
      ? `/accounts/${accountId}/label/${params.label}`
      : params.teamId
        ? `/accounts/${accountId}/team/${params.teamId}`
        : kind === "mentions" || kind === "unattended" || kind === "participating"
          ? `/accounts/${accountId}/${kind}`
          : `/accounts/${accountId}`;
  const nav = useNavigate();
  const [tab, setTab] = useState<Tab>("all");
  const [status, setStatus] = useState("open");
  const [sort, setSort] = useState("last_activity_at_desc");
  const [list, setList] = useState<Conversation[]>([]);
  const [meta, setMeta] = useState<Meta | null>(null);
  const [inboxes, setInboxes] = useState<Record<number, Inbox>>({});
  const [labelColors, setLabelColors] = useState<Record<string, string>>({});
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [loading, setLoading] = useState(true);
  const [epoch, setEpoch] = useState(0);
  const latest = useRef(0);

  useEffect(() => {
    let active = true;
    api<Inbox[] | { payload: Inbox[] }>(`${acct(accountId)}/inboxes`)
      .then((r) => {
        if (!active) return;
        const arr = Array.isArray(r) ? r : r.payload ?? [];
        setInboxes(Object.fromEntries(arr.map((i) => [i.id, i])));
      })
      .catch(() => {});
    api<{ payload: Label[] }>(`${acct(accountId)}/labels`)
      .then((r) => {
        if (!active) return;
        setLabelColors(Object.fromEntries((r.payload ?? []).map((l) => [l.title, l.color])));
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [accountId]);

  const load = useMemo(
    () => async () => {
      const seq = ++latest.current;
      const r = await api<{ data: { meta: Meta; payload: Conversation[] } }>(
        `${acct(accountId)}/conversations?status=${status}&assignee_type=${tab}&sort_by=${sort}${scope}`
      );
      if (seq !== latest.current) return;
      setList(r.data.payload);
      setMeta(r.data.meta);
      setLoading(false);
    },
    [accountId, status, tab, sort, scope]
  );

  useEffect(() => {
    setLoading(true);
    void load();
  }, [load]);

  useEffect(() => {
    const onReconnect = () => {
      void load();
      setEpoch((n) => n + 1);
    };
    window.addEventListener("pulsar:cable-reconnect", onReconnect);
    return () => window.removeEventListener("pulsar:cable-reconnect", onReconnect);
  }, [load]);

  useEffect(
    () =>
      subscribe((e) => {
        if (
          e.event.startsWith("conversation.") ||
          e.event === "message.created" ||
          e.event === "assignee.changed"
        ) {
          void load();
        }
      }),
    [subscribe, load]
  );

  const count = (k: Tab) =>
    meta ? { me: meta.mine_count, unassigned: meta.unassigned_count, all: meta.all_count }[k] : null;

  async function bulkResolve() {
    if (!selectedIds.length) return;
    await api(`${acct(accountId)}/bulk_actions`, {
      method: "POST",
      json: { type: "Conversation", ids: selectedIds, fields: { status: "resolved" } },
    }).catch(() => {});
    setSelectedIds([]);
    void load();
  }

  return (
    <div className="flex min-h-0 flex-1 bg-n-background">
      {/* Left Conversation List */}
      <section
        className={`flex w-full md:w-[22rem] shrink-0 flex-col border-r border-n-weak bg-n-surface-1 ${
          conversationId ? "hidden md:flex" : "flex"
        }`}
        aria-label="Conversations"
      >
        <header className="flex h-14 shrink-0 items-center justify-between gap-2 px-4">
          <div className="flex items-center gap-2">
            <h1 className="text-base font-medium text-n-slate-12">Conversations</h1>
            <span className="rounded-md bg-n-slate-3 px-1.5 py-0.5 font-mono text-[11px] font-medium text-n-slate-11">
              {meta?.all_count ?? list.length}
            </span>
          </div>
          <select
            aria-label="Status"
            value={status}
            onChange={(e) => setStatus(e.target.value)}
            className="h-7 rounded-lg border border-n-weak bg-n-background px-2 text-xs font-medium text-n-slate-12 focus:border-n-brand focus:outline-none"
          >
            <option value="open">Open</option>
            <option value="resolved">Resolved</option>
            <option value="pending">Pending</option>
            <option value="snoozed">Snoozed</option>
            <option value="all">All</option>
          </select>
        </header>

        <div role="tablist" className="flex items-center gap-5 border-b border-n-weak px-4">
          {TABS.map((t) => (
            <button
              key={t.key}
              role="tab"
              aria-selected={tab === t.key}
              onClick={() => setTab(t.key)}
              className={`-mb-px flex h-9 items-center gap-1.5 border-b-2 text-xs font-medium transition-colors ${
                tab === t.key
                  ? "border-n-brand text-n-blue-11"
                  : "border-transparent text-n-slate-10 hover:text-n-slate-12"
              }`}
            >
              {t.label}
              <span className="rounded-full bg-n-slate-3 px-1.5 py-0.5 font-mono text-[10px] text-n-slate-11">
                {count(t.key) ?? 0}
              </span>
            </button>
          ))}
        </div>

        {selectedIds.length > 0 ? (
          <div className="flex items-center justify-between border-b border-n-weak bg-n-blue-3/40 px-4 py-2 text-xs">
            <span className="font-medium text-n-slate-12">{selectedIds.length} selected</span>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => void bulkResolve()}
                className="rounded-md bg-n-brand px-2.5 py-1 font-medium text-white hover:bg-n-blue-10"
              >
                Resolve
              </button>
              <button
                type="button"
                onClick={() => setSelectedIds([])}
                className="text-n-slate-11 hover:text-n-slate-12"
              >
                Clear
              </button>
            </div>
          </div>
        ) : (
          <label className="flex items-center gap-2 border-b border-n-weak px-4 py-1.5 text-xs text-n-slate-10">
            <span>Sort</span>
            <select
              aria-label="Sort conversations"
              value={sort}
              onChange={(e) => setSort(e.target.value)}
              className="min-w-0 flex-1 bg-transparent font-medium text-n-slate-11 focus:outline-none"
            >
              <option value="last_activity_at_desc">Last activity: Newest first</option>
              <option value="last_activity_at_asc">Last activity: Oldest first</option>
              <option value="created_at_desc">Created at: Newest first</option>
              <option value="created_at_asc">Created at: Oldest first</option>
              <option value="priority_desc">Priority: Highest first</option>
              <option value="priority_asc">Priority: Lowest first</option>
              <option value="waiting_since_asc">Pending Response: Longest first</option>
              <option value="waiting_since_desc">Pending Response: Shortest first</option>
            </select>
          </label>
        )}

        <ul className="m-0 min-h-0 flex-1 list-none overflow-y-auto">
          {!loading && list.length === 0 && (
            <li className="p-8 text-center text-xs text-n-slate-10">
              No {status} conversations here
            </li>
          )}
          {list.map((c) => {
            const last = c.last_non_activity_message ?? c.messages[0];
            const active = String(c.id) === conversationId;
            const checked = selectedIds.includes(c.id);
            const inbox = inboxes[c.inbox_id];
            return (
              <li key={c.id} className="group relative border-b border-n-weak">
                <Link
                  to={`${base}/conversations/${c.id}`}
                  className={`flex items-start gap-3 px-4 py-3 transition-colors ${
                    active
                      ? "bg-n-blue-3/50 dark:bg-n-alpha-2"
                      : "hover:bg-n-alpha-1"
                  }`}
                >
                  <div className="relative mt-1 shrink-0">
                    <Avatar
                      name={c.meta.sender.name || "Unknown"}
                      src={c.meta.sender.thumbnail}
                      size={32}
                      status={c.meta.sender.availability_status === "online" ? "online" : undefined}
                    />
                    <label
                      onClick={(e) => e.stopPropagation()}
                      className={`absolute inset-0 flex items-center justify-center rounded-full bg-n-solid-1/85 backdrop-blur-[1px] transition-opacity ${
                        checked ? "opacity-100" : "opacity-0 group-hover:opacity-100"
                      }`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        aria-label={`Select conversation ${c.id}`}
                        onChange={(e) => {
                          e.stopPropagation();
                          setSelectedIds((prev) =>
                            e.target.checked ? [...prev, c.id] : prev.filter((x) => x !== c.id)
                          );
                        }}
                        className="size-4 accent-n-brand"
                      />
                    </label>
                  </div>

                  <div className="min-w-0 flex-1">
                    {/* Channel + Assignee/Priority meta row */}
                    <div className="mb-0.5 flex items-center justify-between gap-2 text-[11px] text-n-slate-10">
                      <span className="inline-flex min-w-0 items-center gap-1 truncate">
                        {channelBadgeIcon(inbox?.channel_type || c.meta.channel)}
                        <span className="truncate">{inbox?.name || `Inbox #${c.inbox_id}`}</span>
                      </span>
                      <span className="num shrink-0 font-mono text-[10px]">
                        {timeAgo(c.timestamp)}
                      </span>
                    </div>

                    <div className="flex items-center justify-between gap-2">
                      <span
                        className={`truncate text-sm text-n-slate-12 ${
                          c.unread_count ? "font-semibold" : "font-medium"
                        }`}
                      >
                        {c.meta.sender.name || "Unknown contact"}
                      </span>
                      {c.unread_count > 0 && (
                        <span className="num inline-grid h-4 min-w-4 shrink-0 place-items-center rounded-full bg-n-teal-9 px-1 text-[10px] font-semibold text-white">
                          {c.unread_count}
                        </span>
                      )}
                    </div>

                    <p className="mt-0.5 flex items-center gap-1 truncate text-xs text-n-slate-11">
                      {last?.private && <Lock className="size-3 shrink-0 text-n-amber-11" />}
                      <span className="truncate">
                        {last?.content || (last?.attachments?.length ? "Attachment" : "No messages yet")}
                      </span>
                    </p>

                    {(c.labels.length > 0 || c.meta.assignee || c.priority) && (
                      <div className="mt-1.5 flex flex-wrap items-center gap-1">
                        {c.priority && (
                          <span className="inline-flex items-center gap-0.5 rounded bg-n-amber-3 px-1.5 py-0.5 text-[10px] font-medium capitalize text-n-amber-11">
                            <AlertCircle className="size-2.5" />
                            {c.priority}
                          </span>
                        )}
                        {c.meta.assignee && (
                          <span className="rounded bg-n-slate-3 px-1.5 py-0.5 text-[10px] text-n-slate-11">
                            {c.meta.assignee.available_name}
                          </span>
                        )}
                        {c.labels.slice(0, 3).map((l) => (
                          <span
                            key={l}
                            className="inline-flex items-center gap-1 rounded border border-n-weak bg-n-background px-1.5 py-0.5 text-[10px] text-n-slate-11"
                          >
                            <span
                              className="size-1.5 rounded-full"
                              style={{ backgroundColor: labelColors[l] || "#3B7AF5" }}
                            />
                            {l}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                </Link>
              </li>
            );
          })}
        </ul>
      </section>

      {/* Center Conversation Pane + Right Sidebar */}
      <section
        className={`flex min-w-0 flex-1 flex-col ${
          conversationId ? "flex" : "hidden md:flex"
        }`}
      >
        {conversationId ? (
          <Thread
            key={conversationId}
            id={Number(conversationId)}
            epoch={epoch}
            accountId={accountId}
            meId={me.id}
            inboxes={inboxes}
            subscribe={subscribe}
            onGone={() => nav(`${base}/conversations`)}
            onBackMobile={() => nav(`${base}/conversations`)}
          />
        ) : (
          <Empty title="Pick a conversation">
            Replies, private notes and files appear here as they arrive
          </Empty>
        )}
      </section>
    </div>
  );
}

export function Thread({
  id,
  epoch,
  accountId,
  meId,
  inboxes = {},
  subscribe,
  onGone,
  onBackMobile,
}: {
  id: number;
  epoch: number;
  accountId: number;
  meId: number;
  inboxes?: Record<number, Inbox>;
  subscribe: ShellCtx["subscribe"];
  onGone: () => void;
  onBackMobile?: () => void;
}) {
  const base = `${acct(accountId)}/conversations/${id}`;
  const [conv, setConv] = useState<Conversation | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [showSidebar, setShowSidebar] = useState(true);
  const [statusMenu, setStatusMenu] = useState(false);
  const [moreMenu, setMoreMenu] = useState(false);
  const [transcriptOpen, setTranscriptOpen] = useState(false);
  const [transcriptEmail, setTranscriptEmail] = useState("");
  const [transcriptStatus, setTranscriptStatus] = useState("");
  const [copiedId, setCopiedId] = useState(false);
  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [captainSummary, setCaptainSummary] = useState("");
  const [error, setError] = useState("");
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let live = true;
    Promise.all([api<Conversation>(base), api<{ payload: Message[] }>(`${base}/messages`)])
      .then(([c, m]) => {
        if (!live) return;
        setConv(c);
        setMessages(m.payload);
        setTranscriptEmail(c.meta.sender.email || "");
        api(`${base}/update_last_seen`, { method: "POST" }).catch(() => {});
      })
      .catch((e) => (e.status === 404 ? onGone() : setError(e.message)));
    return () => {
      live = false;
    };
  }, [base, epoch]);

  useEffect(
    () =>
      subscribe((e) => {
        const d = e.data;
        if ((e.event === "message.created" || e.event === "message.updated") && d.conversation_id === id) {
          setMessages((prev) => {
            const i = prev.findIndex((m) => m.id === d.id || (d.echo_id && m.echo_id === d.echo_id));
            if (i === -1) return [...prev, d];
            const next = prev.slice();
            next[i] = d;
            return next;
          });
        } else if ((e.event.startsWith("conversation.") || e.event === "assignee.changed") && d.id === id) {
          setConv(d);
        }
      }),
    [subscribe, id]
  );

  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [messages.length]);

  async function setConvStatus(next: "open" | "resolved" | "pending" | "snoozed", snoozedUntil?: number) {
    if (!conv) return;
    setStatusMenu(false);
    await api(`${base}/toggle_status`, {
      method: "POST",
      json: snoozedUntil ? { status: next, snoozed_until: snoozedUntil } : { status: next },
    });
    setConv({ ...conv, status: next, snoozed_until: snoozedUntil ?? null });
  }

  async function assignMe() {
    setMoreMenu(false);
    await api(`${base}/assignments`, { method: "POST", json: { assignee_id: meId } });
    setConv(await api<Conversation>(base));
  }

  async function toggleMute() {
    if (!conv) return;
    setMoreMenu(false);
    await api(`${base}/${conv.muted ? "unmute" : "mute"}`, { method: "POST" });
    setConv({ ...conv, muted: !conv.muted });
  }

  async function sendTranscript(e: React.FormEvent) {
    e.preventDefault();
    if (!transcriptEmail.trim()) return;
    setTranscriptStatus("");
    try {
      await api(`${base}/transcript`, { method: "POST", json: { email: transcriptEmail.trim() } });
      setTranscriptOpen(false);
    } catch (err) {
      setTranscriptStatus(err instanceof Error ? err.message : "Could not send transcript");
    }
  }

  async function deleteMessage(mid: number) {
    await api(`${base}/messages/${mid}`, { method: "DELETE" }).catch(() => {});
    setMessages((prev) =>
      prev.map((m) =>
        m.id === mid ? { ...m, content: "This message was deleted", content_attributes: { ...m.content_attributes, deleted: true } } : m
      )
    );
  }

  if (error) return <Empty title="Could not load this conversation">{error}</Empty>;
  if (!conv) return <div className="p-6 text-sm text-n-slate-10">Loading...</div>;

  const inbox = inboxes[conv.inbox_id];
  const msgMap = Object.fromEntries(messages.map((m) => [m.id, m]));

  return (
    <div className="flex min-h-0 flex-1 bg-n-background">
      <div className="flex min-w-0 flex-1 flex-col">
        {/* ConversationHeader matching ConversationHeader.vue */}
        <header className="flex h-12 shrink-0 items-center justify-between gap-3 border-b border-n-weak bg-n-surface-1 px-4">
          <div className="flex min-w-0 items-center gap-2.5">
            {onBackMobile && (
              <button
                type="button"
                onClick={onBackMobile}
                className="rounded-md px-1.5 py-1 text-xs text-n-slate-11 hover:bg-n-alpha-2 md:hidden"
              >
                ← Back
              </button>
            )}
            <Avatar
              name={conv.meta.sender.name || "Unknown"}
              src={conv.meta.sender.thumbnail}
              size={32}
              status={conv.meta.sender.availability_status === "online" ? "online" : undefined}
            />
            <div className="min-w-0">
              <div className="flex items-center gap-1.5">
                <p className="truncate text-sm font-medium leading-tight text-n-slate-12">
                  {conv.meta.sender.name || "Unknown contact"}
                </p>
                {conv.muted && (
                  <span title="Muted" className="text-n-slate-10">
                    <BellOff className="size-3.5" />
                  </span>
                )}
              </div>
              <div className="flex items-center gap-1.5 text-xs text-n-slate-10">
                <button
                  type="button"
                  onClick={() => {
                    navigator.clipboard?.writeText(String(conv.id)).catch(() => {});
                    setCopiedId(true);
                    setTimeout(() => setCopiedId(false), 1200);
                  }}
                  className="inline-flex items-center gap-1 font-mono hover:text-n-slate-12"
                >
                  #{conv.id}
                  {copiedId && <Check className="size-3 text-n-teal-11" />}
                </button>
                {inbox && (
                  <>
                    <span>•</span>
                    <span className="truncate">{inbox.name}</span>
                  </>
                )}
                {conv.status === "snoozed" && (
                  <>
                    <span>•</span>
                    <span className="inline-flex items-center gap-1 font-medium text-n-amber-11">
                      <Clock className="size-3" /> Snoozed
                    </span>
                  </>
                )}
              </div>
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-1.5">
            {conv.meta.assignee?.id !== meId && (
              <button
                type="button"
                onClick={() => void assignMe()}
                className="hidden h-8 items-center gap-1.5 rounded-lg border border-n-weak bg-n-background px-2.5 text-xs font-medium text-n-slate-12 hover:bg-n-alpha-2 sm:inline-flex"
              >
                <UserCheck className="size-3.5 text-n-slate-11" />
                Assign to me
              </button>
            )}

            {/* Resolve Split Button */}
            <div className="relative inline-flex rounded-lg shadow-xs">
              <button
                type="button"
                onClick={() => void setConvStatus(conv.status === "open" ? "resolved" : "open")}
                className={`inline-flex h-8 items-center gap-1.5 rounded-l-lg px-3 text-xs font-medium transition ${
                  conv.status === "open"
                    ? "bg-n-brand text-white hover:bg-n-blue-10"
                    : "border border-r-0 border-n-weak bg-n-background text-n-slate-12 hover:bg-n-alpha-2"
                }`}
              >
                {conv.status === "open" ? (
                  <CheckCircle2 className="size-3.5" />
                ) : (
                  <RotateCcw className="size-3.5" />
                )}
                {conv.status === "open" ? "Resolve" : "Reopen"}
              </button>
              <button
                type="button"
                aria-label="More resolve options"
                onClick={() => {
                  setStatusMenu((v) => !v);
                  setMoreMenu(false);
                }}
                className={`grid h-8 w-7 place-items-center rounded-r-lg border-l transition ${
                  conv.status === "open"
                    ? "border-white/20 bg-n-brand text-white hover:bg-n-blue-10"
                    : "border border-n-weak bg-n-background text-n-slate-11 hover:bg-n-alpha-2"
                }`}
              >
                <ChevronDown className="size-3.5" />
              </button>

              {statusMenu && (
                <div className="absolute right-0 top-full mt-1 z-40 w-48 rounded-xl border border-n-weak bg-n-solid-1 p-1 shadow-xl text-xs">
                  <button
                    type="button"
                    onClick={() => void setConvStatus("pending")}
                    className="block w-full rounded-lg px-2.5 py-1.5 text-left text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    Mark as Pending
                  </button>
                  <button
                    type="button"
                    onClick={() => void setConvStatus("snoozed", Math.floor(Date.now() / 1000) + 3600)}
                    className="block w-full rounded-lg px-2.5 py-1.5 text-left text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    Snooze for 1 hour
                  </button>
                  <button
                    type="button"
                    onClick={() => void setConvStatus("snoozed", Math.floor(Date.now() / 1000) + 86400)}
                    className="block w-full rounded-lg px-2.5 py-1.5 text-left text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    Snooze until tomorrow
                  </button>
                  <button
                    type="button"
                    onClick={() => void setConvStatus("snoozed", Math.floor(Date.now() / 1000) + 7 * 86400)}
                    className="block w-full rounded-lg px-2.5 py-1.5 text-left text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    Snooze until next week
                  </button>
                </div>
              )}
            </div>

            {/* More Actions (Mute, Send Transcript) */}
            <div className="relative">
              <button
                type="button"
                aria-label="Conversation actions"
                onClick={() => {
                  setMoreMenu((v) => !v);
                  setStatusMenu(false);
                }}
                className="grid size-8 place-items-center rounded-lg text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
              >
                <MoreVertical className="size-4" />
              </button>
              {moreMenu && (
                <div className="absolute right-0 top-full mt-1 z-40 w-48 rounded-xl border border-n-weak bg-n-solid-1 p-1 shadow-xl text-xs">
                  <button
                    type="button"
                    onClick={() => void toggleMute()}
                    className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    <BellOff className="size-3.5 text-n-slate-10" />
                    {conv.muted ? "Unmute conversation" : "Mute conversation"}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setMoreMenu(false);
                      setTranscriptOpen(true);
                    }}
                    className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    <Send className="size-3.5 text-n-slate-10" />
                    Send transcript
                  </button>
                  <button
                    type="button"
                    onClick={() => void assignMe()}
                    className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    <UserCheck className="size-3.5 text-n-slate-10" />
                    Assign to me
                  </button>
                </div>
              )}
            </div>

            <button
              type="button"
              onClick={() => setShowSidebar((v) => !v)}
              aria-label="Toggle contact panel"
              className={`hidden xl:grid size-8 place-items-center rounded-lg transition ${
                showSidebar
                  ? "bg-n-alpha-2 text-n-slate-12"
                  : "text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
              }`}
            >
              <PanelRight className="size-4" />
            </button>
          </div>
        </header>

        {/* Transcript Modal */}
        {transcriptOpen && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-n-overlay-default px-4">
            <form
              onSubmit={sendTranscript}
              className="w-full max-w-md space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-5 shadow-xl"
            >
              <div className="flex items-center justify-between">
                <h2 className="text-base font-semibold text-n-slate-12">Send Conversation Transcript</h2>
                <button type="button" onClick={() => setTranscriptOpen(false)} className="text-n-slate-10 hover:text-n-slate-12">
                  <X className="size-4" />
                </button>
              </div>
              <label className="block space-y-1 text-xs font-medium text-n-slate-11">
                Recipient Email
                <input
                  required
                  type="email"
                  value={transcriptEmail}
                  onChange={(e) => setTranscriptEmail(e.target.value)}
                  placeholder="contact@example.com"
                  className="h-9 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 focus:border-n-brand focus:outline-none"
                />
              </label>
              {transcriptStatus && <p className="text-xs text-n-ruby-11">{transcriptStatus}</p>}
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setTranscriptOpen(false)}
                  className="h-8 rounded-lg px-3 text-xs text-n-slate-11 hover:bg-n-alpha-2"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="h-8 rounded-lg bg-n-brand px-3 text-xs font-medium text-white hover:bg-n-blue-10"
                >
                  Send Transcript
                </button>
              </div>
            </form>
          </div>
        )}

        {/* Messages Thread */}
        <div className="min-h-0 flex-1 overflow-y-auto bg-n-background px-4 py-5 sm:px-6">
          <ol className="mx-auto flex max-w-4xl flex-col gap-3 list-none">
            {messages.map((m, idx) => {
              const prev = idx > 0 ? messages[idx - 1] : null;
              const prevDay = prev ? dayDividerLabel(prev.created_at) : null;
              const curDay = dayDividerLabel(m.created_at);
              const showDate = curDay !== prevDay;
              const quoted = m.content_attributes?.in_reply_to ? msgMap[m.content_attributes.in_reply_to] : undefined;

              return (
                <div key={m.id}>
                  {showDate && (
                    <div className="my-3 flex items-center justify-center">
                      <span className="rounded-full border border-n-weak bg-n-solid-1 px-3 py-0.5 text-[11px] font-medium text-n-slate-10 shadow-2xs">
                        {curDay}
                      </span>
                    </div>
                  )}
                  <Bubble
                    m={m}
                    quoted={quoted}
                    onReply={() => setReplyTo(m)}
                    onDelete={() => void deleteMessage(m.id)}
                  />
                </div>
              );
            })}
          </ol>
          {captainSummary && (
            <div className="mx-auto mt-4 max-w-4xl rounded-xl border border-n-blue-5 bg-n-blue-2 p-3.5 text-xs text-n-slate-12 dark:bg-n-solid-blue-2">
              <div className="mb-1 flex items-center justify-between font-semibold text-n-blue-11">
                <span>Captain Thread Summary</span>
                <button type="button" onClick={() => setCaptainSummary("")} className="text-n-slate-10 hover:text-n-slate-12">
                  <X className="size-3.5" />
                </button>
              </div>
              <p className="whitespace-pre-wrap leading-relaxed">{captainSummary}</p>
            </div>
          )}
          <div ref={end} />
        </div>

        <ReplyBox
          base={base}
          accountId={accountId}
          conversationDisplayId={conv.id}
          inboxId={conv.inbox_id}
          inReplyTo={replyTo}
          onClearReplyTo={() => setReplyTo(null)}
          onCaptainSummary={setCaptainSummary}
          onSent={(m) => setMessages((prev) => (prev.some((x) => x.id === m.id) ? prev : [...prev, m]))}
        />
      </div>

      {showSidebar && (
        <ConversationSidebar
          accountId={accountId}
          conversation={conv}
          inboxName={inbox?.name}
          onChange={setConv}
          onClose={() => setShowSidebar(false)}
        />
      )}
    </div>
  );
}

function Bubble({
  m,
  quoted,
  onReply,
  onDelete,
}: {
  m: Message;
  quoted?: Message;
  onReply: () => void;
  onDelete: () => void;
}) {
  const [copied, setCopied] = useState(false);

  if (m.message_type === 2) {
    return (
      <li className="py-1 text-center">
        <span className="inline-flex items-center gap-1.5 rounded-full bg-n-slate-3/60 px-3 py-0.5 text-xs text-n-slate-11">
          {m.content}
          <span className="font-mono text-[10px] text-n-slate-10">{clock(m.created_at)}</span>
        </span>
      </li>
    );
  }

  const out = m.message_type === 1;
  const tone = m.private
    ? "bg-n-solid-amber border border-n-amber-6/50 text-n-slate-12"
    : out
      ? "bg-n-brand text-white"
      : "bg-n-solid-2 border border-n-weak text-n-slate-12";

  return (
    <li className={`group flex items-end gap-2 ${out ? "justify-end" : "justify-start"}`}>
      {!out && (
        <div className="mb-1 shrink-0">
          <Avatar
            size={24}
            name={m.sender?.name || "Contact"}
            src={m.sender?.thumbnail}
          />
        </div>
      )}

      <div className={`relative max-w-[36rem] rounded-2xl px-3.5 py-2.5 shadow-2xs ${tone} ${out ? "rounded-br-sm" : "rounded-bl-sm"}`}>
        {/* Hover Quick Toolbar */}
        {!m.content_attributes?.deleted && (
          <div
            className={`pointer-events-none absolute -top-3 ${
              out ? "right-2" : "left-2"
            } z-10 flex items-center gap-0.5 rounded-lg border border-n-weak bg-n-solid-1 px-1 py-0.5 text-n-slate-11 opacity-0 shadow-sm transition group-hover:pointer-events-auto group-hover:opacity-100`}
          >
            <button
              type="button"
              onClick={onReply}
              title="Reply to message"
              className="grid size-6 place-items-center rounded hover:bg-n-alpha-2 hover:text-n-slate-12"
            >
              <Reply className="size-3" />
            </button>
            {m.content && (
              <button
                type="button"
                onClick={() => {
                  navigator.clipboard?.writeText(m.content || "").catch(() => {});
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1200);
                }}
                title="Copy message"
                className="grid size-6 place-items-center rounded hover:bg-n-alpha-2 hover:text-n-slate-12"
              >
                {copied ? <Check className="size-3 text-n-teal-11" /> : <Copy className="size-3" />}
              </button>
            )}
            <button
              type="button"
              onClick={onDelete}
              title="Delete message"
              className="grid size-6 place-items-center rounded hover:bg-n-ruby-3 hover:text-n-ruby-11"
            >
              <Trash2 className="size-3" />
            </button>
          </div>
        )}

        {quoted && (
          <div
            className={`mb-2 rounded-lg border-l-2 px-2.5 py-1 text-xs ${
              out && !m.private
                ? "border-white/50 bg-white/10 text-white/90"
                : "border-n-brand bg-n-alpha-1 text-n-slate-11"
            }`}
          >
            <div className="font-medium">{quoted.sender?.name || "Message"}</div>
            <div className="truncate opacity-80">{quoted.content || "Attachment"}</div>
          </div>
        )}

        {m.content && (
          <p
            className={`whitespace-pre-wrap break-words text-sm leading-relaxed ${
              m.content_attributes?.deleted ? "italic opacity-65" : ""
            }`}
          >
            {m.content}
          </p>
        )}

        {m.attachments?.map((a) =>
          a.file_type === "image" ? (
            <a key={a.id} href={a.data_url} target="_blank" rel="noreferrer" className="mt-2 block">
              <img src={a.thumb_url || a.data_url} alt="Attached image" className="max-h-64 rounded-lg object-cover" />
            </a>
          ) : a.file_type === "audio" ? (
            <audio key={a.id} controls src={a.data_url} className="mt-2 max-w-full" />
          ) : (
            <a
              key={a.id}
              href={a.data_url}
              target="_blank"
              rel="noreferrer"
              className="mt-2 flex items-center gap-2 text-xs underline-offset-2 hover:underline"
            >
              <Paperclip className="size-3.5" />
              <span>
                {a.extension?.toUpperCase() ?? "File"} <span className="font-mono opacity-75">{bytes(a.file_size)}</span>
              </span>
            </a>
          )
        )}

        <p
          className={`mt-1 flex items-center gap-1 font-mono text-[10px] ${
            out && !m.private ? "text-white/75" : "text-n-slate-10"
          }`}
        >
          {m.private && <Lock className="size-2.5" />}
          {m.sender?.name && `${m.sender.name} • `}
          {clock(m.created_at)}
        </p>
      </div>
    </li>
  );
}
