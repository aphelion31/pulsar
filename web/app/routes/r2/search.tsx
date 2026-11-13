// Global search. mirrors chatwoot/app/javascript/dashboard/routes/dashboard/search (tabs All/Contacts/Conversations/Messages).
// Backend: GET /api/v1/accounts/:id/search?q= returns { payload: { contacts, conversations, messages, articles } }.
import { useEffect, useState } from "react";
import { Link, useNavigate, useOutletContext, useParams } from "react-router";
import { Avatar, Empty, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { timeAgo } from "~/lib/format";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Contact = { id: number; name: string | null; email: string | null; phone_number: string | null };
type ConvHit = { id: number; created_at: number; contact: { name: string | null; email: string | null }; inbox: { id: number; name: string | null } };
type MsgHit = { id: number; content: string | null; conversation_id: number; created_at: number; private: boolean };
type Payload = { contacts: Contact[]; conversations: ConvHit[]; messages: MsgHit[]; articles: unknown[] };

const TABS = [
  { key: "all", label: "SEARCH.TABS.ALL" },
  { key: "contacts", label: "SEARCH.TABS.CONTACTS" },
  { key: "conversations", label: "SEARCH.TABS.CONVERSATIONS" },
  { key: "messages", label: "SEARCH.TABS.MESSAGES" },
] as const;
type TabKey = (typeof TABS)[number]["key"];

const empty: Payload = { contacts: [], conversations: [], messages: [], articles: [] };

export default function SearchPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { tab: rawTab } = useParams();
  const nav = useNavigate();
  const tab: TabKey = (TABS.find((x) => x.key === rawTab)?.key ?? "all") as TabKey;
  const [query, setQuery] = useState("");
  const [term, setTerm] = useState("");
  const [data, setData] = useState<Payload>(empty);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  // debounce the query; only search with 3+ characters (Chatwoot INPUT_PLACEHOLDER rule)
  useEffect(() => {
    const id = setTimeout(() => setTerm(query.trim()), 300);
    return () => clearTimeout(id);
  }, [query]);

  useEffect(() => {
    if (term.length < 3) {
      setData(empty);
      setError("");
      return;
    }
    let live = true;
    setLoading(true);
    api<{ payload: Payload }>(`${acct(accountId)}/search?q=${encodeURIComponent(term)}`)
      .then((r) => live && setData({ ...empty, ...r.payload }))
      .catch((e) => live && setError(e instanceof Error ? e.message : "Request failed"))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [accountId, term]);

  const show = (k: "contacts" | "conversations" | "messages") => tab === "all" || tab === k;
  const none = term.length >= 3 && !loading && !error &&
    !data.contacts.length && !data.conversations.length && !data.messages.length;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title={t("SEARCH.TABS.ALL")} />
      <div className="flex flex-col gap-4 border-b border-line px-6 py-3">
        <input
          aria-label={t("SEARCH.INPUT_PLACEHOLDER")}
          placeholder={t("SEARCH.INPUT_PLACEHOLDER")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="h-9 w-full max-w-xl rounded-lg border border-line-strong bg-surface px-3 text-[14px] outline-none focus:border-accent"
        />
        <div role="tablist" className="flex gap-4">
          {TABS.map((x) => (
            <button
              key={x.key}
              role="tab"
              aria-selected={tab === x.key}
              onClick={() => nav(`/accounts/${accountId}/search${x.key === "all" ? "" : `/${x.key}`}`)}
              className={`-mb-px h-9 border-b-2 text-[13px] ${tab === x.key ? "border-accent text-ink" : "border-transparent text-ink-3 hover:text-ink-2"}`}
            >
              {t(x.label)}
            </button>
          ))}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-6">
        {term.length < 3 && <Empty title={t("SEARCH.EMPTY_STATE_DEFAULT")} />}
        {term.length >= 3 && loading && <p className="text-[13px] text-ink-3">{t("SEARCH.SEARCHING_DATA")}</p>}
        {error && <p role="alert" className="text-[13px] text-bad">{error}</p>}
        {none && <Empty title={t("SEARCH.EMPTY_STATE_FULL", { query: term })} />}

        {show("contacts") && data.contacts.length > 0 && (
          <section className="mb-6">
            <h2 className="mb-2 text-[13px] font-medium text-ink-2">{t("SEARCH.SECTION.CONTACTS")}</h2>
            <ul className="divide-y divide-line rounded-lg border border-line">
              {data.contacts.map((c) => (
                <li key={c.id}>
                  <Link to={`/accounts/${accountId}/contacts/${c.id}`} className="flex items-center gap-3 px-4 py-3 hover:bg-bg">
                    <Avatar name={c.name} size={28} />
                    <span className="min-w-0 truncate text-[13px] font-medium">{c.name || c.email || c.phone_number || "-"}</span>
                    <span className="ml-auto truncate text-[12px] text-ink-3">{c.email || c.phone_number || ""}</span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        )}

        {show("conversations") && data.conversations.length > 0 && (
          <section className="mb-6">
            <h2 className="mb-2 text-[13px] font-medium text-ink-2">{t("SEARCH.SECTION.CONVERSATIONS")}</h2>
            <ul className="divide-y divide-line rounded-lg border border-line">
              {data.conversations.map((c) => (
                <li key={c.id}>
                  <Link to={`/accounts/${accountId}/conversations/${c.id}`} className="flex items-center gap-3 px-4 py-3 hover:bg-bg">
                    <span className="num font-mono text-[12px] text-ink-3">#{c.id}</span>
                    <span className="min-w-0 truncate text-[13px] font-medium">{c.contact?.name || c.contact?.email || "-"}</span>
                    <span className="ml-auto truncate text-[12px] text-ink-3">{c.inbox?.name}</span>
                    <span className="num shrink-0 font-mono text-[11px] text-ink-3">{timeAgo(c.created_at)}</span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        )}

        {show("messages") && data.messages.length > 0 && (
          <section className="mb-6">
            <h2 className="mb-2 text-[13px] font-medium text-ink-2">{t("SEARCH.SECTION.MESSAGES")}</h2>
            <ul className="divide-y divide-line rounded-lg border border-line">
              {data.messages.map((m) => (
                <li key={m.id}>
                  <Link to={`/accounts/${accountId}/conversations/${m.conversation_id}`} className="flex items-center gap-3 px-4 py-3 hover:bg-bg">
                    <span className="min-w-0 flex-1 truncate text-[13px]">{m.content}</span>
                    {m.private && <span className="shrink-0 text-[11px] text-ink-3">{t("SEARCH.PRIVATE")}</span>}
                    <span className="num shrink-0 font-mono text-[11px] text-ink-3">{timeAgo(m.created_at)}</span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </div>
  );
}
