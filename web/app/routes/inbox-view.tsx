import { useCallback, useEffect, useState } from "react";
import { Bell, CheckCheck, Clock, Trash2 } from "lucide-react";
import { Link, useNavigate, useOutletContext, useParams } from "react-router";
import { Button, Empty, ErrorText, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { timeAgo } from "~/lib/format";
import type { ShellCtx } from "./shell";

type NotificationActor = {
  id: number;
  inbox_id?: number;
  status?: string;
  contact?: { name?: string | null } | null;
};

type NotificationItem = {
  id: number;
  notification_type: string;
  primary_actor_type: string;
  primary_actor_id: number;
  primary_actor?: NotificationActor | null;
  read_at: string | number | null;
  snoozed_until: string | number | null;
  last_activity_at?: number | null;
  created_at?: number | null;
};

type NotificationResponse = {
  data: {
    payload: NotificationItem[];
    meta: { count: number; unread_count: number; current_page?: number };
  };
};

type ConversationDetail = {
  id: number;
  status: string;
  inbox_id: number;
  meta?: { sender?: { name?: string | null; email?: string | null } };
  messages?: { id: number; content: string | null; message_type: number; private: boolean; created_at: number }[];
};

export default function InboxViewRoute() {
  const { accountId, subscribe } = useOutletContext<ShellCtx>();
  const { id: selectedId } = useParams();
  const navigate = useNavigate();
  const base = `${acct(accountId)}/notifications`;

  const [items, setItems] = useState<NotificationItem[]>([]);
  const [meta, setMeta] = useState({ count: 0, unread_count: 0 });
  const [page, setPage] = useState(1);
  const [includeRead, setIncludeRead] = useState(false);
  const [includeSnoozed, setIncludeSnoozed] = useState(false);
  const [sortOrder, setSortOrder] = useState<"desc" | "asc">("desc");
  const [version, setVersion] = useState(0);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [preview, setPreview] = useState<ConversationDetail | null>(null);
  const [reply, setReply] = useState("");

  const refresh = useCallback(() => setVersion((v) => v + 1), []);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const stop = subscribe((e) => {
      if (!e.event.startsWith("notification.") || Number(e.data?.account_id) !== accountId) return;
      if (timer) clearTimeout(timer);
      timer = setTimeout(refresh, 50);
    });
    const onReconnect = () => refresh();
    window.addEventListener("pulsar:cable-reconnect", onReconnect);
    return () => {
      stop();
      if (timer) clearTimeout(timer);
      window.removeEventListener("pulsar:cable-reconnect", onReconnect);
    };
  }, [accountId, subscribe, refresh]);

  useEffect(() => {
    const controller = new AbortController();
    const q = new URLSearchParams({ page: String(page), sort_order: sortOrder });
    if (includeRead) q.append("includes[]", "read");
    if (includeSnoozed) q.append("includes[]", "snoozed");
    setLoading(true);
    api<NotificationResponse>(`${base}?${q}`, { signal: controller.signal })
      .then((res) => {
        if (controller.signal.aborted) return;
        setItems(res.data.payload);
        setMeta(res.data.meta);
        setError("");
        if (!res.data.payload.length && page > 1) setPage((p) => p - 1);
      })
      .catch((e) => {
        if (!controller.signal.aborted) setError(e instanceof Error ? e.message : "Failed to load notifications");
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => {
      controller.abort();
    };
  }, [base, page, includeRead, includeSnoozed, sortOrder, version]);

  const activeItem = items.find((n) => String(n.id) === selectedId) ?? items[0] ?? null;
  const activeConvId = activeItem?.primary_actor?.id ?? (selectedId ? Number(selectedId) : null);

  useEffect(() => {
    if (!activeConvId) {
      setPreview(null);
      return;
    }
    let alive = true;
    api<ConversationDetail>(`${acct(accountId)}/conversations/${activeConvId}`)
      .then((c) => {
        if (alive) setPreview(c);
      })
      .catch(() => {
        if (alive) setPreview(null);
      });
    return () => {
      alive = false;
    };
  }, [accountId, activeConvId, version]);

  async function mutate(path: string, method: string, json?: unknown) {
    setBusy(true);
    setError("");
    try {
      await api(`${base}${path}`, { method, json });
      refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed");
    } finally {
      setBusy(false);
    }
  }

  async function sendReply(e: React.FormEvent) {
    e.preventDefault();
    if (!activeConvId || !reply.trim()) return;
    setBusy(true);
    try {
      await api(`${acct(accountId)}/conversations/${activeConvId}/messages`, {
        method: "POST",
        json: { content: reply.trim(), private: false },
      });
      setReply("");
      refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to send reply");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <PageHeader title={`My Inbox (${meta.unread_count} unread)`}>
        <Button variant="quiet" disabled={busy} onClick={() => mutate("/read_all", "POST")}>
          <CheckCheck className="size-4" />
          Mark all read
        </Button>
        <Button
          variant="quiet"
          disabled={busy}
          onClick={() => {
            if (window.confirm("Delete all read notifications?")) {
              mutate("/destroy_all", "POST", { type: "read" });
            }
          }}
        >
          <Trash2 className="size-4" />
          Delete read
        </Button>
      </PageHeader>

      <div className="flex min-h-0 flex-1">
        <div className="flex w-96 shrink-0 flex-col border-r border-line">
          <div className="flex flex-wrap items-center gap-3 border-b border-line px-4 py-2.5 text-[12px] text-ink-2">
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={includeRead}
                onChange={(e) => {
                  setIncludeRead(e.target.checked);
                  setPage(1);
                }}
              />
              Read
            </label>
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={includeSnoozed}
                onChange={(e) => {
                  setIncludeSnoozed(e.target.checked);
                  setPage(1);
                }}
              />
              Snoozed
            </label>
            <select
              aria-label="Sort order"
              value={sortOrder}
              onChange={(e) => setSortOrder(e.target.value as "desc" | "asc")}
              className="ml-auto rounded border border-line bg-surface px-2 py-1 text-[12px]"
            >
              <option value="desc">Newest</option>
              <option value="asc">Oldest</option>
            </select>
          </div>

          <div className="px-4 pt-2">
            <ErrorText>{error}</ErrorText>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto divide-y divide-line">
            {loading && items.length === 0 ? (
              <p className="p-4 text-sm text-ink-3">Loading notifications...</p>
            ) : items.length === 0 ? (
              <Empty title="All caught up">
                <p className="text-sm">No notifications match your filters.</p>
              </Empty>
            ) : (
              items.map((n) => {
                const selected = activeItem?.id === n.id;
                const convId = n.primary_actor?.id ?? n.primary_actor_id;
                return (
                  <div
                    key={n.id}
                    className={`cursor-pointer p-3.5 transition-colors hover:bg-raised ${selected ? "bg-accent-soft" : ""}`}
                    onClick={() => navigate(`/accounts/${accountId}/inbox-view/notification/${n.id}`)}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <p className="flex items-center gap-1.5 text-[13px] font-medium text-ink">
                          {!n.read_at && <span className="size-2 shrink-0 rounded-full bg-accent" />}
                          <span className="truncate capitalize">
                            {n.notification_type.replaceAll("_", " ")}
                          </span>
                        </p>
                        <p className="mt-0.5 text-[12px] text-ink-2">
                          Conversation #{convId}
                          {n.primary_actor?.contact?.name ? ` · ${n.primary_actor.contact.name}` : ""}
                        </p>
                      </div>
                      {n.created_at && (
                        <span className="shrink-0 text-[11px] text-ink-3">{timeAgo(n.created_at)}</span>
                      )}
                    </div>
                    <div className="mt-2 flex flex-wrap gap-2 text-[11px]" onClick={(e) => e.stopPropagation()}>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => mutate(n.read_at ? `/${n.id}/unread` : `/${n.id}`, n.read_at ? "POST" : "PATCH")}
                        className="rounded border border-line px-2 py-0.5 text-ink-2 hover:bg-surface"
                      >
                        {n.read_at ? "Mark unread" : "Mark read"}
                      </button>
                      {!n.snoozed_until && (
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() =>
                            mutate(`/${n.id}/snooze`, "POST", {
                              snoozed_until: Math.floor(Date.now() / 1000) + 3600,
                            })
                          }
                          className="inline-flex items-center gap-1 rounded border border-line px-2 py-0.5 text-ink-2 hover:bg-surface"
                        >
                          <Clock className="size-3" />
                          Snooze 1h
                        </button>
                      )}
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => mutate(`/${n.id}`, "DELETE")}
                        className="rounded border border-line px-2 py-0.5 text-bad hover:bg-surface"
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                );
              })
            )}
          </div>

          <nav className="flex items-center justify-between border-t border-line px-4 py-2 text-[12px]" aria-label="Notification pages">
            <Button variant="quiet" disabled={page <= 1} onClick={() => setPage((p) => p - 1)}>
              Previous
            </Button>
            <span className="text-ink-3">Page {page}</span>
            <Button variant="quiet" disabled={page * 15 >= meta.count} onClick={() => setPage((p) => p + 1)}>
              Next
            </Button>
          </nav>
        </div>

        <div className="flex min-w-0 flex-1 flex-col">
          {preview ? (
            <>
              <div className="flex items-center justify-between border-b border-line px-6 py-3">
                <div>
                  <h2 className="text-sm font-medium text-ink">
                    {preview.meta?.sender?.name || `Conversation #${preview.id}`}
                  </h2>
                  <p className="text-xs text-ink-3">
                    {preview.meta?.sender?.email || `Status: ${preview.status}`}
                  </p>
                </div>
                <Link
                  to={`/accounts/${accountId}/conversations/${preview.id}`}
                  className="rounded-lg border border-line px-3 py-1.5 text-xs font-medium text-ink hover:bg-raised"
                >
                  Open in Conversations
                </Link>
              </div>
              <div className="flex-1 space-y-3 overflow-y-auto p-6">
                {(preview.messages ?? []).map((m) => (
                  <div
                    key={m.id}
                    className={`max-w-xl rounded-lg border px-4 py-2.5 text-sm ${
                      m.private
                        ? "border-amber-300 bg-amber-50 text-amber-950"
                        : m.message_type === 1
                          ? "ml-auto border-accent/20 bg-accent-soft text-ink"
                          : "border-line bg-bg text-ink"
                    }`}
                  >
                    <p className="whitespace-pre-wrap">{m.content || "(attachment)"}</p>
                  </div>
                ))}
              </div>
              <form onSubmit={sendReply} className="flex gap-2 border-t border-line p-4">
                <input
                  value={reply}
                  onChange={(e) => setReply(e.target.value)}
                  placeholder="Write a quick reply..."
                  className="h-9 flex-1 rounded-lg border border-line-strong bg-surface px-3 text-sm outline-none focus:border-accent"
                />
                <Button type="submit" disabled={busy || !reply.trim()}>
                  Send
                </Button>
              </form>
            </>
          ) : (
            <Empty title="Select a notification">
              <Bell className="mx-auto mt-2 size-6 text-ink-3" />
              <p className="mt-1 text-sm">Pick any notification on the left to inspect and reply.</p>
            </Empty>
          )}
        </div>
      </div>
    </section>
  );
}
