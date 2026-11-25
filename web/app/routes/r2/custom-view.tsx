// Custom view (saved folder) conversations. mirrors chatwoot/app/javascript/dashboard/routes/dashboard/customviews
// and conversation.routes.js (custom_view + conversation). Loads GET /custom_filters/:id, then
// POST /conversations/filter {payload} with the saved query. Layout matches routes/inbox.tsx.
import { useEffect, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import { Avatar, Empty } from "~/components/ui";
import { acct, api, type Conversation } from "~/lib/api";
import { timeAgo } from "~/lib/format";
import type { ShellCtx } from "../shell";
import { Thread } from "../inbox";

type CustomFilter = { id: number; name: string; query: { payload: unknown[] } };

export default function CustomViewRoute() {
  const { accountId, me, subscribe } = useOutletContext<ShellCtx>();
  const { id, conversation_id: conversationIdParam } = useParams();
  const conversationId = conversationIdParam;
  const base = `/accounts/${accountId}/custom_view/${id}`;
  const [filter, setFilter] = useState<CustomFilter | null>(null);
  const [list, setList] = useState<Conversation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [epoch, setEpoch] = useState(0);

  useEffect(() => {
    let live = true;
    setLoading(true);
    setError("");
    (async () => {
      const f = await api<CustomFilter>(`${acct(accountId)}/custom_filters/${id}`);
      const r = await api<{ payload: Conversation[] }>(`${acct(accountId)}/conversations/filter`, {
        method: "POST",
        json: { payload: f.query?.payload ?? [] },
      });
      if (!live) return;
      setFilter(f);
      setList(r.payload);
    })()
      .catch((e) => live && setError(e instanceof Error ? e.message : "Request failed"))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [accountId, id, epoch]);

  // any conversation or message event refreshes the folder list
  useEffect(
    () =>
      subscribe((e) => {
        if (e.event.startsWith("conversation.") || e.event === "message.created" || e.event === "assignee.changed") setEpoch((n) => n + 1);
      }),
    [subscribe],
  );

  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[22rem] shrink-0 flex-col border-r border-line" aria-label={filter?.name ?? "Folder"}>
        <header className="flex h-14 shrink-0 items-center px-4">
          <h1 className="truncate text-[15px] font-medium">{filter?.name ?? ""}</h1>
        </header>
        <ul className="min-h-0 flex-1 overflow-y-auto">
          {error && <li className="p-6 text-[13px] text-bad" role="alert">{error}</li>}
          {!loading && !error && list.length === 0 && <li className="p-6 text-ink-3">No conversations here</li>}
          {list.map((c) => {
            const active = String(c.id) === conversationId;
            return (
              <li key={c.id}>
                <Link to={`${base}/conversations/${c.id}`} className={`flex gap-3 border-b border-line px-4 py-3 transition-colors duration-150 ${active ? "bg-accent-soft" : "hover:bg-bg"}`}>
                  <Avatar name={c.meta.sender.name} size={32} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className="truncate text-[13px] font-medium">{c.meta.sender.name || "Unknown contact"}</span>
                      <span className="num shrink-0 font-mono text-[11px] text-ink-3">{timeAgo(c.timestamp)}</span>
                    </div>
                    <p className="mt-0.5 truncate text-[13px] text-ink-2">{c.messages[0]?.content || "No messages yet"}</p>
                  </div>
                </Link>
              </li>
            );
          })}
        </ul>
      </section>
      <section className="flex min-w-0 flex-1 flex-col">
        {conversationId ? (
          <Thread
            key={conversationId}
            id={Number(conversationId)}
            epoch={epoch}
            accountId={accountId}
            meId={me.id}
            subscribe={subscribe}
            onGone={() => undefined}
          />
        ) : (
          <Empty title="Pick a conversation">Replies, private notes and files appear here as they arrive</Empty>
        )}
      </section>
    </div>
  );
}
