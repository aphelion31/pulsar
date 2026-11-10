// Inbox: conversation list (left), thread + composer (right). Realtime via shell cable.
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useOutletContext, useParams } from "react-router";
import { CheckCircle2, Lock, Paperclip, RotateCcw, Send } from "lucide-react";
import { Avatar, Button, Empty } from "~/components/ui";
import { acct, api, type Conversation, type Message } from "~/lib/api";
import { bytes, clock, timeAgo } from "~/lib/format";
import type { ShellCtx } from "./shell";

const TABS = [
  { key: "me", label: "Mine" },
  { key: "unassigned", label: "Unassigned" },
  { key: "all", label: "All" },
] as const;
type Tab = (typeof TABS)[number]["key"];
type Meta = { mine_count: number; unassigned_count: number; all_count: number };

export default function InboxRoute() {
  const { me, accountId, subscribe } = useOutletContext<ShellCtx>();
  const { conversationId } = useParams();
  const nav = useNavigate();
  const [tab, setTab] = useState<Tab>("all");
  const [status, setStatus] = useState<"open" | "resolved">("open");
  const [list, setList] = useState<Conversation[]>([]);
  const [meta, setMeta] = useState<Meta | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useMemo(
    () => async () => {
      const r = await api<{ data: { meta: Meta; payload: Conversation[] } }>(
        `${acct(accountId)}/conversations?status=${status}&assignee_type=${tab}&sort_by=last_activity_at_desc`,
      );
      setList(r.data.payload);
      setMeta(r.data.meta);
      setLoading(false);
    },
    [accountId, status, tab],
  );

  useEffect(() => {
    setLoading(true);
    load();
  }, [load]);

  // any conversation or message event refreshes the list (cheap: one query, 25 rows)
  useEffect(
    () => subscribe((e) => {
      if (e.event.startsWith("conversation.") || e.event === "message.created" || e.event === "assignee.changed") load();
    }),
    [subscribe, load],
  );

  const count = (k: Tab) => (meta ? { me: meta.mine_count, unassigned: meta.unassigned_count, all: meta.all_count }[k] : null);

  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[22rem] shrink-0 flex-col border-r border-line" aria-label="Conversations">
        <header className="flex h-14 shrink-0 items-center justify-between px-4">
          <h1 className="text-[15px] font-medium">Conversations</h1>
          <select
            aria-label="Status"
            value={status}
            onChange={(e) => setStatus(e.target.value as "open" | "resolved")}
            className="h-8 rounded-lg border border-line-strong bg-surface px-2 text-[13px] text-ink-2"
          >
            <option value="open">Open</option>
            <option value="resolved">Resolved</option>
          </select>
        </header>
        <div role="tablist" className="flex gap-4 border-b border-line px-4">
          {TABS.map((t) => (
            <button
              key={t.key}
              role="tab"
              aria-selected={tab === t.key}
              onClick={() => setTab(t.key)}
              className={`-mb-px flex h-9 items-center gap-1.5 border-b-2 text-[13px] transition-colors duration-150 ${
                tab === t.key ? "border-accent text-ink" : "border-transparent text-ink-3 hover:text-ink-2"
              }`}
            >
              {t.label}
              <span className="num font-mono text-[11px] text-ink-3">{count(t.key) ?? ""}</span>
            </button>
          ))}
        </div>
        <ul className="min-h-0 flex-1 overflow-y-auto">
          {!loading && list.length === 0 && (
            <li className="p-6 text-ink-3">No {status} conversations here</li>
          )}
          {list.map((c) => {
            const last = c.last_non_activity_message ?? c.messages[0];
            const active = String(c.id) === conversationId;
            return (
              <li key={c.id}>
                <Link
                  to={`/accounts/${accountId}/conversations/${c.id}`}
                  className={`flex gap-3 border-b border-line px-4 py-3 transition-colors duration-150 ${active ? "bg-accent-soft" : "hover:bg-bg"}`}
                >
                  <Avatar name={c.meta.sender.name} size={32} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className={`truncate text-[13px] ${c.unread_count ? "font-semibold" : "font-medium"}`}>
                        {c.meta.sender.name || "Unknown contact"}
                      </span>
                      <span className="num shrink-0 font-mono text-[11px] text-ink-3">{timeAgo(c.timestamp)}</span>
                    </div>
                    <p className="mt-0.5 truncate text-[13px] text-ink-2">
                      {last?.private && <Lock size={12} strokeWidth={1.5} className="mr-1 inline align-[-1px]" />}
                      {last?.content || (last?.attachments?.length ? "Attachment" : "No messages yet")}
                    </p>
                    {(c.labels.length > 0 || c.unread_count > 0) && (
                      <div className="mt-1.5 flex items-center gap-1.5">
                        {c.labels.slice(0, 3).map((l) => (
                          <span key={l} className="rounded-md bg-raised px-1.5 py-0.5 text-[11px] text-ink-2">
                            {l}
                          </span>
                        ))}
                        {c.unread_count > 0 && (
                          <span className="num ml-auto rounded-full bg-accent px-1.5 text-[11px] font-medium text-accent-ink">
                            {c.unread_count}
                          </span>
                        )}
                      </div>
                    )}
                  </div>
                </Link>
              </li>
            );
          })}
        </ul>
      </section>
      <section className="flex min-w-0 flex-1 flex-col">
        {conversationId ? (
          <Thread key={conversationId} id={Number(conversationId)} accountId={accountId} meId={me.id} subscribe={subscribe} onGone={() => nav(`/accounts/${accountId}/conversations`)} />
        ) : (
          <Empty title="Pick a conversation">Replies, private notes and files appear here as they arrive</Empty>
        )}
      </section>
    </div>
  );
}

function Thread({ id, accountId, meId, subscribe, onGone }: { id: number; accountId: number; meId: number; subscribe: ShellCtx["subscribe"]; onGone: () => void }) {
  const base = `${acct(accountId)}/conversations/${id}`;
  const [conv, setConv] = useState<Conversation | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [error, setError] = useState("");
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let live = true;
    Promise.all([api<Conversation>(base), api<{ payload: Message[] }>(`${base}/messages`)])
      .then(([c, m]) => {
        if (!live) return;
        setConv(c);
        setMessages(m.payload);
        api(`${base}/update_last_seen`, { method: "POST" }).catch(() => {});
      })
      .catch((e) => (e.status === 404 ? onGone() : setError(e.message)));
    return () => {
      live = false;
    };
  }, [base]);

  useEffect(
    () => subscribe((e) => {
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
    [subscribe, id],
  );

  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [messages.length]);

  async function toggleStatus() {
    if (!conv) return;
    const next = conv.status === "open" ? "resolved" : "open";
    await api(`${base}/toggle_status`, { method: "POST", json: { status: next } });
    setConv({ ...conv, status: next });
  }

  async function assignMe() {
    await api(`${base}/assignments`, { method: "POST", json: { assignee_id: meId } });
  }

  if (error) return <Empty title="Could not load this conversation">{error}</Empty>;
  if (!conv) return <div className="p-6 text-ink-3">Loading</div>;

  return (
    <>
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-line px-6">
        <div className="flex min-w-0 items-center gap-3">
          <Avatar name={conv.meta.sender.name} size={28} />
          <div className="min-w-0">
            <p className="truncate text-[14px] font-medium">{conv.meta.sender.name || "Unknown contact"}</p>
            <p className="truncate text-[12px] text-ink-3">
              <span className="num font-mono">#{conv.id}</span>
              {conv.meta.assignee ? `, assigned to ${conv.meta.assignee.available_name}` : ", unassigned"}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-1">
          {conv.meta.assignee?.id !== meId && (
            <Button variant="quiet" onClick={assignMe}>
              Assign to me
            </Button>
          )}
          <Button variant={conv.status === "open" ? "primary" : "quiet"} onClick={toggleStatus}>
            {conv.status === "open" ? <CheckCircle2 size={16} strokeWidth={1.5} /> : <RotateCcw size={16} strokeWidth={1.5} />}
            {conv.status === "open" ? "Resolve" : "Reopen"}
          </Button>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto bg-bg px-6 py-6">
        <ol className="mx-auto grid max-w-3xl gap-3">
          {messages.map((m) => (
            <Bubble key={m.id} m={m} />
          ))}
        </ol>
        <div ref={end} />
      </div>
      <Composer base={base} onSent={(m) => setMessages((prev) => (prev.some((x) => x.id === m.id) ? prev : [...prev, m]))} />
    </>
  );
}

function Bubble({ m }: { m: Message }) {
  if (m.message_type === 2) {
    return (
      <li className="py-1 text-center text-[12px] text-ink-3">
        {m.content} <span className="num font-mono">{clock(m.created_at)}</span>
      </li>
    );
  }
  const out = m.message_type === 1;
  const tone = m.private
    ? "bg-note border border-note-line text-ink"
    : out
      ? "bg-accent text-accent-ink"
      : "bg-surface border border-line text-ink";
  return (
    <li className={`flex ${out ? "justify-end" : "justify-start"}`}>
      <div className={`max-w-[34rem] rounded-xl px-3.5 py-2.5 ${tone}`}>
        {m.content && <p className="whitespace-pre-wrap break-words text-[14px] leading-relaxed">{m.content}</p>}
        {m.attachments?.map((a) =>
          a.file_type === "image" ? (
            <a key={a.id} href={a.data_url} target="_blank" rel="noreferrer" className="mt-2 block">
              <img src={a.thumb_url || a.data_url} alt="Attached image" className="max-h-64 rounded-lg object-cover" />
            </a>
          ) : (
            <a key={a.id} href={a.data_url} target="_blank" rel="noreferrer" className="mt-2 flex items-center gap-2 underline-offset-2 hover:underline">
              <Paperclip size={14} strokeWidth={1.5} />
              <span className="text-[13px]">
                {a.extension?.toUpperCase() ?? "File"} <span className="num font-mono opacity-70">{bytes(a.file_size)}</span>
              </span>
            </a>
          ),
        )}
        <p className={`num mt-1 flex items-center gap-1 font-mono text-[11px] ${out && !m.private ? "text-white/70" : "text-ink-3"}`}>
          {m.private && <Lock size={11} strokeWidth={1.5} />}
          {m.sender?.name && `${m.sender.name}, `}
          {clock(m.created_at)}
        </p>
      </div>
    </li>
  );
}

function Composer({ base, onSent }: { base: string; onSent: (m: Message) => void }) {
  const [text, setText] = useState("");
  const [note, setNote] = useState(false);
  const [files, setFiles] = useState<File[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const fileInput = useRef<HTMLInputElement>(null);

  async function send() {
    const content = text;
    if (!content.trim() && files.length === 0) return;
    setBusy(true);
    setError("");
    setText("");
    try {
      const echo_id = crypto.randomUUID();
      let m: Message;
      if (files.length) {
        const fd = new FormData();
        if (content.trim()) fd.append("content", content);
        fd.append("private", String(note));
        fd.append("echo_id", echo_id);
        files.forEach((f) => fd.append("attachments[]", f));
        m = await api<Message>(`${base}/messages`, { method: "POST", body: fd });
      } else {
        m = await api<Message>(`${base}/messages`, { method: "POST", json: { content, private: note, echo_id } });
      }
      onSent(m);
      setFiles([]);
    } catch (e) {
      setText(content);
      setError(e instanceof Error ? e.message : "Could not send");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={`shrink-0 border-t border-line px-6 py-4 ${note ? "bg-note" : "bg-surface"}`}>
      <div className="mx-auto max-w-3xl">
        <div role="tablist" className="mb-2 flex gap-1">
          {[
            { v: false, l: "Reply" },
            { v: true, l: "Private note" },
          ].map((o) => (
            <button
              key={o.l}
              role="tab"
              aria-selected={note === o.v}
              onClick={() => setNote(o.v)}
              className={`h-7 rounded-md px-2.5 text-[12px] transition-colors duration-150 ${note === o.v ? "bg-raised text-ink" : "text-ink-3 hover:text-ink-2"}`}
            >
              {o.l}
            </button>
          ))}
        </div>
        <textarea
          aria-label={note ? "Private note" : "Reply"}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) send();
          }}
          rows={3}
          placeholder={note ? "Only your team sees this" : "Write a reply"}
          className="w-full resize-none rounded-lg border border-line-strong bg-surface px-3 py-2 text-[14px] outline-none transition-colors duration-150 placeholder:text-ink-3 focus:border-accent"
        />
        {files.length > 0 && (
          <ul className="mt-2 flex flex-wrap gap-2">
            {files.map((f, i) => (
              <li key={i} className="flex items-center gap-2 rounded-md bg-raised px-2 py-1 text-[12px] text-ink-2">
                {f.name} <span className="num font-mono text-ink-3">{bytes(f.size)}</span>
                <button aria-label={`Remove ${f.name}`} className="text-ink-3 hover:text-ink" onClick={() => setFiles(files.filter((_, j) => j !== i))}>
                  ×
                </button>
              </li>
            ))}
          </ul>
        )}
        {error && <p role="alert" className="mt-2 text-[13px] text-bad">{error}</p>}
        <div className="mt-2 flex items-center justify-between">
          <input ref={fileInput} type="file" multiple hidden onChange={(e) => setFiles([...files, ...Array.from(e.target.files ?? [])])} />
          <Button variant="quiet" onClick={() => fileInput.current?.click()} aria-label="Attach files">
            <Paperclip size={16} strokeWidth={1.5} /> Attach
          </Button>
          <div className="flex items-center gap-3">
            <span className="hidden text-[12px] text-ink-3 sm:inline">
              <kbd className="rounded border border-line-strong px-1 font-mono text-[11px]">⌘</kbd>{" "}
              <kbd className="rounded border border-line-strong px-1 font-mono text-[11px]">Enter</kbd>
            </span>
            <Button onClick={send} disabled={busy || (!text.trim() && files.length === 0)}>
              <Send size={16} strokeWidth={1.5} /> {note ? "Add note" : "Send"}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
