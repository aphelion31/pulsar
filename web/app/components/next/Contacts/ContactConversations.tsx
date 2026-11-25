import { useState, useEffect, useCallback } from "react";
import { Link } from "react-router";
import Spinner from "~/components/next/Spinner";
import { api, acct } from "~/lib/api";
import { timeAgo } from "~/lib/format";
import { t } from "~/lib/i18n";

export type ContactConversationItem = {
  id: number;
  inbox_id: number;
  status: string;
  timestamp: number;
  created_at: number;
  labels: string[];
  account_id: number;
};

type Props = {
  accountId: number;
  contactId: number;
};

export default function ContactConversations({ accountId, contactId }: Props) {
  const [conversations, setConversations] = useState<ContactConversationItem[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState("");

  const base = `${acct(accountId)}/contacts/${contactId}/conversations`;

  const fetchConversations = useCallback(async () => {
    setIsLoading(true);
    try {
      const res = await api<{ payload: ContactConversationItem[] }>(base);
      setConversations(res.payload ?? []);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to load conversations");
    } finally {
      setIsLoading(false);
    }
  }, [base]);

  useEffect(() => {
    fetchConversations();
  }, [fetchConversations]);

  const statusColor = (status: string) => {
    switch (status) {
      case "open":
        return "bg-n-teal-9/10 text-n-teal-11 border-n-teal-8/20";
      case "resolved":
        return "bg-n-slate-9/10 text-n-slate-11 border-n-slate-8/20";
      case "pending":
        return "bg-n-amber-9/10 text-n-amber-11 border-n-amber-8/20";
      case "snoozed":
        return "bg-n-iris-9/10 text-n-iris-11 border-n-iris-8/20";
      default:
        return "bg-n-slate-9/10 text-n-slate-11 border-n-slate-8/20";
    }
  };

  return (
    <div className="flex flex-col gap-4 px-6">
      {error && (
        <div className="rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-xs text-n-ruby-11">
          {error}
        </div>
      )}

      {isLoading ? (
        <div className="flex items-center justify-center py-8 text-n-slate-11">
          <Spinner size={24} />
        </div>
      ) : conversations.length === 0 ? (
        <p className="py-8 text-sm text-center text-n-slate-11">
          {t("CONTACTS_LAYOUT.SIDEBAR.HISTORY.EMPTY_STATE")}
        </p>
      ) : (
        <div className="flex flex-col gap-2.5">
          {conversations.map((conv) => (
            <Link
              key={conv.id}
              to={`/accounts/${accountId}/conversations/${conv.id}`}
              className="flex items-center justify-between p-3.5 rounded-xl border border-n-weak bg-n-surface-1 hover:border-n-strong hover:bg-n-solid-2 transition-all duration-150 group"
            >
              <div className="flex flex-col gap-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-n-slate-12 group-hover:text-n-brand transition-colors">
                    #{conv.id}
                  </span>
                  <span
                    className={`px-2 py-0.5 text-2xs uppercase tracking-wide font-medium rounded-md border ${statusColor(
                      conv.status
                    )}`}
                  >
                    {conv.status}
                  </span>
                </div>
                {conv.labels.length > 0 && (
                  <div className="flex flex-wrap gap-1 mt-1">
                    {conv.labels.map((label) => (
                      <span
                        key={label}
                        className="px-1.5 py-0.5 text-2xs rounded bg-n-alpha-2 text-n-slate-11"
                      >
                        {label}
                      </span>
                    ))}
                  </div>
                )}
              </div>

              <span className="text-xs text-n-slate-10 shrink-0">
                {timeAgo(conv.timestamp || conv.created_at)}
              </span>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
