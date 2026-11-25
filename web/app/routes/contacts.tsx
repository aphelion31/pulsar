import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useLocation, useNavigate, useOutletContext, useParams, useSearchParams } from "react-router";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import Spinner from "~/components/next/Spinner";
import ContactHeader from "~/components/next/Contacts/ContactHeader";
import ContactsCard from "~/components/next/Contacts/ContactsCard";
import PaginationFooter from "~/components/next/Contacts/PaginationFooter";
import CreateContactDialog from "~/components/next/Contacts/CreateContactDialog";
import ConfirmContactDeleteDialog from "~/components/next/Contacts/ConfirmContactDeleteDialog";
import type { ContactItem } from "~/components/next/Contacts/types";
import type { FilterCondition } from "~/components/next/Contacts/ContactFilter";
import type { ShellCtx } from "./shell";

type Listing = {
  meta: { count: number; current_page: number; has_more?: boolean };
  payload: ContactItem[];
};

// mirrors chatwoot custom filter (segment) record: query.payload is the filter condition list
type Segment = {
  id: number;
  name: string;
  query?: { payload?: FilterCondition[] };
};

export default function Contacts() {
  const { accountId } = useOutletContext<ShellCtx>();
  const navigate = useNavigate();
  const location = useLocation();
  const params = useParams();
  const [searchParams, setSearchParams] = useSearchParams();

  // mirrors chatwoot/app/javascript/dashboard/routes/dashboard/contacts/routes.js
  const segmentId = params.segmentId;
  const labelName = params.label;
  const isActiveView = /\/contacts\/active\/?$/.test(location.pathname);
  const [segment, setSegment] = useState<Segment | null>(null);

  const initialPage = Number(searchParams.get("page")) || 1;
  const initialSearch = searchParams.get("search") || "";

  const [page, setPage] = useState(initialPage);
  const [search, setSearch] = useState(initialSearch);
  const [debouncedSearch, setDebouncedSearch] = useState(initialSearch);
  const [activeSort, setActiveSort] = useState("last_activity_at");
  const [activeOrdering, setActiveOrdering] = useState("-");
  const [appliedFilters, setAppliedFilters] = useState<FilterCondition[]>([]);

  const [list, setList] = useState<Listing | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);

  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [isCreating, setIsCreating] = useState(false);

  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [contactToDelete, setContactToDelete] = useState<ContactItem | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  const [isUpdatingId, setIsUpdatingId] = useState<number | null>(null);

  // Reset to page 1 when switching between all / active / label / segment views
  const viewKey = `${isActiveView}|${segmentId ?? ""}|${labelName ?? ""}`;
  const prevViewKey = useRef(viewKey);
  useEffect(() => {
    if (prevViewKey.current !== viewKey) {
      prevViewKey.current = viewKey;
      setPage(1);
    }
  }, [viewKey]);

  // Load segment (custom filter) definition for segment view
  useEffect(() => {
    if (!segmentId) {
      setSegment(null);
      return;
    }
    let cancelled = false;
    setSegment(null);
    api<Segment>(`${acct(accountId)}/custom_filters/${segmentId}`)
      .then((data) => {
        if (!cancelled) setSegment(data);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : "Failed to load segment");
        setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [accountId, segmentId]);

  // Debounce search input
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(search);
      setPage(1);
    }, 300);
    return () => clearTimeout(timer);
  }, [search]);

  // Keep search params synced
  useEffect(() => {
    const nextParams = new URLSearchParams();
    if (page > 1) nextParams.set("page", String(page));
    if (debouncedSearch) nextParams.set("search", debouncedSearch);
    setSearchParams(nextParams, { replace: true });
  }, [page, debouncedSearch, setSearchParams]);

  const fullSort = useMemo(() => {
    return `${activeOrdering}${activeSort}`;
  }, [activeOrdering, activeSort]);

  const base = `${acct(accountId)}/contacts`;

  const fetchContacts = useCallback(async () => {
    setIsLoading(true);
    setError("");

    // segment definition still loading
    if (!debouncedSearch && segmentId && !segment) return;

    try {
      let data: Listing;
      if (debouncedSearch) {
        data = await api<Listing>(
          `${base}/search?q=${encodeURIComponent(debouncedSearch)}&page=${page}&sort=${fullSort}`
        );
      } else if (isActiveView) {
        data = await api<Listing>(`${base}/active?page=${page}&sort=${fullSort}`);
      } else if (segmentId && segment) {
        data = await api<Listing>(`${base}/filter?page=${page}&sort=${fullSort}`, {
          method: "POST",
          json: { payload: segment.query?.payload ?? [] },
        });
      } else if (labelName) {
        data = await api<Listing>(
          `${base}?page=${page}&sort=${fullSort}&labels[]=${encodeURIComponent(labelName)}`
        );
      } else if (appliedFilters.length > 0) {
        data = await api<Listing>(`${base}/filter?page=${page}&sort=${fullSort}`, {
          method: "POST",
          json: { payload: appliedFilters },
        });
      } else {
        data = await api<Listing>(`${base}?page=${page}&sort=${fullSort}`);
      }
      setList(data);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to load contacts");
    } finally {
      setIsLoading(false);
    }
  }, [base, debouncedSearch, appliedFilters, page, fullSort, isActiveView, segmentId, segment, labelName]);

  useEffect(() => {
    fetchContacts();
  }, [fetchContacts, revision]);

  const handleUpdateContact = async (updated: Partial<ContactItem>) => {
    if (!updated.id) return;
    setIsUpdatingId(updated.id);
    try {
      await api<{ payload: ContactItem }>(`${base}/${updated.id}`, {
        method: "PATCH",
        json: updated,
      });
      setRevision((n) => n + 1);
    } finally {
      setIsUpdatingId(null);
    }
  };

  const handleCreateContact = async (contactData: Partial<ContactItem>) => {
    setIsCreating(true);
    try {
      await api<{ payload: { contact: ContactItem } }>(base, {
        method: "POST",
        json: contactData,
      });
      setRevision((n) => n + 1);
    } finally {
      setIsCreating(false);
    }
  };

  const handleDeleteContact = async () => {
    if (!contactToDelete) return;
    setIsDeleting(true);
    try {
      await api(`${base}/${contactToDelete.id}`, { method: "DELETE" });
      setDeleteDialogOpen(false);
      setContactToDelete(null);
      setRevision((n) => n + 1);
    } catch (err: unknown) {
      alert(err instanceof Error ? err.message : "Failed to delete contact");
    } finally {
      setIsDeleting(false);
    }
  };

  const headerTitle = useMemo(() => {
    if (debouncedSearch) return t("CONTACTS_LAYOUT.HEADER.SEARCH_TITLE");
    if (isActiveView) return t("CONTACTS_LAYOUT.HEADER.ACTIVE_TITLE");
    if (segmentId) return segment?.name ?? "";
    if (labelName) return `#${labelName}`;
    return t("CONTACTS_LAYOUT.HEADER.TITLE");
  }, [debouncedSearch, isActiveView, segmentId, segment, labelName]);

  const totalItems = list?.meta.count ?? 0;
  const contacts = list?.payload ?? [];

  return (
    <div className="flex flex-col flex-1 h-full min-w-0 bg-n-surface-1 overflow-hidden">
      <ContactHeader
        headerTitle={headerTitle}
        searchValue={search}
        onSearch={setSearch}
        activeSort={activeSort}
        activeOrdering={activeOrdering}
        onSortChange={({ sort, order }) => {
          setActiveSort(sort);
          setActiveOrdering(order);
          setPage(1);
        }}
        hasActiveFilters={appliedFilters.length > 0}
        onApplyFilters={(filters) => {
          setAppliedFilters(filters);
          setPage(1);
        }}
        onClearFilters={() => {
          setAppliedFilters([]);
          setPage(1);
        }}
        onNewContact={() => setCreateDialogOpen(true)}
      />

      <main className="flex-1 overflow-y-auto px-6 py-4 min-h-0">
        <div className="w-full mx-auto max-w-5xl">
          {error && (
            <div className="mb-4 rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-sm text-n-ruby-11">
              {error}
            </div>
          )}

          {isLoading ? (
            <div className="flex items-center justify-center py-16 text-n-slate-11">
              <Spinner size={28} />
            </div>
          ) : contacts.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-16 gap-3 text-center">
              <p className="text-base font-medium text-n-slate-12">
                {debouncedSearch || appliedFilters.length > 0
                  ? t("CONTACTS_LAYOUT.EMPTY_STATE.TITLE")
                  : t("CONTACTS_LAYOUT.EMPTY_STATE.LIST_EMPTY_STATE_TITLE")}
              </p>
              <p className="text-sm text-n-slate-11 max-w-md">
                {t("CONTACTS_LAYOUT.EMPTY_STATE.SUBTITLE")}
              </p>
            </div>
          ) : (
            <div className="flex flex-col gap-3 pb-6">
              {contacts.map((c) => (
                <ContactsCard
                  key={c.id}
                  contact={c}
                  onUpdate={handleUpdateContact}
                  onDelete={(target) => {
                    setContactToDelete(target);
                    setDeleteDialogOpen(true);
                  }}
                  onViewDetails={(id) => {
                    // keep view prefix: /contacts/:id/segments/:segmentId or /contacts/:id/labels/:label
                    const sub = segmentId
                      ? `/segments/${segmentId}`
                      : labelName
                        ? `/labels/${encodeURIComponent(labelName)}`
                        : "";
                    navigate(`/accounts/${accountId}/contacts/${id}${sub}${location.search}`);
                  }}
                  isUpdating={isUpdatingId === c.id}
                />
              ))}
            </div>
          )}
        </div>
      </main>

      <footer className="shrink-0">
        <PaginationFooter
          currentPage={page}
          totalItems={totalItems}
          itemsPerPage={15}
          onPageChange={setPage}
        />
      </footer>

      <CreateContactDialog
        open={createDialogOpen}
        onClose={() => setCreateDialogOpen(false)}
        onCreate={handleCreateContact}
        isLoading={isCreating}
      />

      <ConfirmContactDeleteDialog
        open={deleteDialogOpen}
        contact={contactToDelete}
        onClose={() => {
          setDeleteDialogOpen(false);
          setContactToDelete(null);
        }}
        onConfirm={handleDeleteContact}
        isLoading={isDeleting}
      />
    </div>
  );
}
