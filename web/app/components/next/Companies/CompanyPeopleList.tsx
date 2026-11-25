import { useState, useEffect, useCallback } from "react";
import { useNavigate } from "react-router";
import Avatar from "~/components/next/Avatar";
import Button from "~/components/next/Button";
import Spinner from "~/components/next/Spinner";
import CompanyPaginationFooter from "./CompanyPaginationFooter";
import { api, acct } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { CompanyContactItem } from "./types";

type Props = {
  accountId: number;
  companyId: number;
  query: string;
  revision: number;
  onContactsChanged: () => void;
};

export default function CompanyPeopleList({
  accountId,
  companyId,
  query,
  revision,
  onContactsChanged,
}: Props) {
  const navigate = useNavigate();
  const [contacts, setContacts] = useState<CompanyContactItem[]>([]);
  const [page, setPage] = useState(1);
  const [totalCount, setTotalCount] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [removingId, setRemovingId] = useState<number | null>(null);
  const [error, setError] = useState("");

  const fetchPeople = useCallback(async () => {
    setIsLoading(true);
    setError("");
    try {
      const qParam = query.trim() ? `&q=${encodeURIComponent(query.trim())}` : "";
      const res = await api<{
        payload: CompanyContactItem[];
        meta: { total_count: number; page: number };
      }>(`${acct(accountId)}/companies/${companyId}/contacts?page=${page}${qParam}`);
      setContacts(res.payload ?? []);
      setTotalCount(res.meta?.total_count ?? 0);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to load people");
    } finally {
      setIsLoading(false);
    }
  }, [accountId, companyId, page, query]);

  useEffect(() => {
    setPage(1);
  }, [query]);

  useEffect(() => {
    fetchPeople();
  }, [fetchPeople, revision]);

  const handleRemove = async (contactId: number) => {
    setRemovingId(contactId);
    try {
      await api(`${acct(accountId)}/companies/${companyId}/contacts/${contactId}`, {
        method: "DELETE",
      });
      onContactsChanged();
      await fetchPeople();
    } catch (err: unknown) {
      alert(
        err instanceof Error
          ? err.message
          : t("COMPANIES.DETAIL.CONTACTS.MESSAGES.REMOVE_ERROR")
      );
    } finally {
      setRemovingId(null);
    }
  };

  if (isLoading && contacts.length === 0) {
    return (
      <div className="flex justify-center py-12 text-n-slate-11">
        <Spinner size={24} />
      </div>
    );
  }

  return (
    <section className="flex flex-col gap-4">
      {error && (
        <div className="rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-xs text-n-ruby-11">
          {error}
        </div>
      )}

      {contacts.length === 0 ? (
        <p className="px-4 py-10 text-sm text-center border border-dashed rounded-xl border-n-weak text-n-slate-11">
          {query.trim()
            ? t("COMPANIES.DETAIL.PEOPLE.NO_MATCHES")
            : t("COMPANIES.DETAIL.PEOPLE.EMPTY")}
        </p>
      ) : (
        <div className="divide-y divide-n-weak">
          {contacts.map((c) => {
            const displayName =
              c.name || c.email || t("COMPANIES.DETAIL.CONTACTS.UNNAMED_CONTACT");
            const attrs = (c.additional_attributes ?? {}) as Record<string, any>;
            const location = [attrs.city, attrs.country].filter(Boolean).join(", ");
            const meta = [c.email, c.phone_number, location].filter(Boolean);

            return (
              <div key={c.id} className="flex items-start justify-between gap-4 py-4">
                <div className="flex items-start flex-1 min-w-0 gap-4">
                  <Avatar
                    name={displayName}
                    src={c.thumbnail || ""}
                    size={40}
                    hideOfflineStatus
                  />
                  <div className="flex flex-col flex-1 min-w-0 gap-1">
                    <button
                      type="button"
                      className="block p-0 truncate text-sm font-medium text-start text-n-slate-12 hover:text-n-blue-11 transition-colors"
                      onClick={() => navigate(`/accounts/${accountId}/contacts/${c.id}`)}
                    >
                      {displayName}
                    </button>
                    {meta.length > 0 && (
                      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-xs text-n-slate-11">
                        {meta.map((item, idx) => (
                          <span key={idx} className="inline-flex items-center gap-1.5">
                            {idx > 0 && <span className="w-px h-3 bg-n-slate-6 mr-1" />}
                            <span className="truncate">{item}</span>
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
                <Button
                  icon="i-lucide-unlink"
                  color="slate"
                  variant="ghost"
                  size="sm"
                  className="hover:enabled:text-n-ruby-11 hover:enabled:bg-n-ruby-2 shrink-0"
                  isLoading={removingId === c.id}
                  disabled={removingId === c.id}
                  onClick={() => handleRemove(c.id)}
                  aria-label={t("COMPANIES.DETAIL.CONTACTS.ACTIONS.REMOVE")}
                />
              </div>
            );
          })}
        </div>
      )}

      {totalCount > 25 && (
        <CompanyPaginationFooter
          currentPage={page}
          totalItems={totalCount}
          itemsPerPage={25}
          onPageChange={setPage}
          className="!px-0 bg-transparent"
        />
      )}
    </section>
  );
}
