import { useState, useEffect, useCallback, useMemo } from "react";
import { useNavigate, useOutletContext, useSearchParams } from "react-router";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import Spinner from "~/components/next/Spinner";
import CompanyHeader from "~/components/next/Companies/CompanyHeader";
import CompaniesCard from "~/components/next/Companies/CompaniesCard";
import CompanyCreateDialog from "~/components/next/Companies/CompanyCreateDialog";
import CompanyPaginationFooter from "~/components/next/Companies/CompanyPaginationFooter";
import type { CompanyItem } from "~/components/next/Companies/types";
import type { ShellCtx } from "./shell";

type Listing = {
  payload: CompanyItem[];
  meta: { total_count: number; page: number };
};

export default function CompaniesPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const initialPage = Number(searchParams.get("page")) || 1;
  const initialSearch = searchParams.get("search") || "";

  const [page, setPage] = useState(initialPage);
  const [search, setSearch] = useState(initialSearch);
  const [debouncedSearch, setDebouncedSearch] = useState(initialSearch);
  const [activeSort, setActiveSort] = useState("name");
  const [activeOrdering, setActiveOrdering] = useState("");

  const [list, setList] = useState<Listing | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState("");

  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [isCreating, setIsCreating] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(search);
      setPage(1);
    }, 300);
    return () => clearTimeout(timer);
  }, [search]);

  useEffect(() => {
    const nextParams = new URLSearchParams();
    if (page > 1) nextParams.set("page", String(page));
    if (debouncedSearch) nextParams.set("search", debouncedSearch);
    setSearchParams(nextParams, { replace: true });
  }, [page, debouncedSearch, setSearchParams]);

  const fullSort = useMemo(() => `${activeOrdering}${activeSort}`, [activeOrdering, activeSort]);

  const base = `${acct(accountId)}/companies`;

  const fetchCompanies = useCallback(async () => {
    setIsLoading(true);
    setError("");
    try {
      const url = debouncedSearch
        ? `${base}/search?q=${encodeURIComponent(debouncedSearch)}&page=${page}&sort=${fullSort}`
        : `${base}?page=${page}&sort=${fullSort}`;
      const data = await api<Listing>(url);
      setList(data);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to load companies");
    } finally {
      setIsLoading(false);
    }
  }, [base, debouncedSearch, page, fullSort]);

  useEffect(() => {
    fetchCompanies();
  }, [fetchCompanies]);

  const handleCreateCompany = async (payload: {
    name: string;
    domain: string | null;
    description: string | null;
  }) => {
    setIsCreating(true);
    try {
      const res = await api<{ payload: CompanyItem }>(base, {
        method: "POST",
        json: payload,
      });
      navigate(`/accounts/${accountId}/companies/${res.payload.id}`);
    } finally {
      setIsCreating(false);
    }
  };

  const companies = list?.payload ?? [];
  const totalItems = list?.meta.total_count ?? 0;

  return (
    <div className="flex flex-col flex-1 h-full min-w-0 bg-n-surface-1 overflow-hidden">
      <CompanyHeader
        headerTitle={t("COMPANIES.HEADER")}
        searchValue={search}
        onSearch={setSearch}
        activeSort={activeSort}
        activeOrdering={activeOrdering}
        onSortChange={({ sort, order }) => {
          setActiveSort(sort);
          setActiveOrdering(order);
          setPage(1);
        }}
        onCreate={() => setCreateDialogOpen(true)}
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
          ) : companies.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-16 gap-3 text-center">
              <p className="text-base font-medium text-n-slate-12">
                {t("COMPANIES.EMPTY_STATE.TITLE")}
              </p>
            </div>
          ) : (
            <div className="divide-y divide-n-weak pb-6">
              {companies.map((c) => (
                <CompaniesCard
                  key={c.id}
                  company={c}
                  onShowCompany={(id) =>
                    navigate(`/accounts/${accountId}/companies/${id}`)
                  }
                />
              ))}
            </div>
          )}
        </div>
      </main>

      <footer className="shrink-0">
        <CompanyPaginationFooter
          currentPage={page}
          totalItems={totalItems}
          itemsPerPage={50}
          onPageChange={setPage}
        />
      </footer>

      <CompanyCreateDialog
        open={createDialogOpen}
        onClose={() => setCreateDialogOpen(false)}
        onCreate={handleCreateCompany}
        isLoading={isCreating}
      />
    </div>
  );
}
