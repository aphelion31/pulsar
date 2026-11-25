import { useState, useEffect, useCallback, useMemo } from "react";
import { useParams, useNavigate, useOutletContext, Link } from "react-router";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import Button from "~/components/next/Button";
import Icon from "~/components/next/Icon";
import Input from "~/components/next/Input";
import Spinner from "~/components/next/Spinner";
import TabBar from "~/components/next/TabBar";
import CompanyDetailHeader from "~/components/next/Companies/CompanyDetailHeader";
import CompanyFacts from "~/components/next/Companies/CompanyFacts";
import CompanyPeopleList from "~/components/next/Companies/CompanyPeopleList";
import CompanyEditPanel from "~/components/next/Companies/CompanyEditPanel";
import CompanyAddContactDialog from "~/components/next/Companies/CompanyAddContactDialog";
import ConfirmCompanyDeleteDialog from "~/components/next/Companies/ConfirmCompanyDeleteDialog";
import type { CompanyItem } from "~/components/next/Companies/types";
import type { ShellCtx } from "./shell";

export default function CompanyDetailPage() {
  const { accountId, me } = useOutletContext<ShellCtx>();
  const params = useParams();
  const navigate = useNavigate();

  const companyId = Number(params.companyId);
  const isAdmin =
    me.accounts.find((a) => a.id === accountId)?.role === "administrator";

  const [company, setCompany] = useState<CompanyItem | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState("");
  const [peopleRevision, setPeopleRevision] = useState(0);

  const [activeTab, setActiveTab] = useState<"people" | "conversations" | "notes">(
    "people"
  );
  const [peopleSearch, setPeopleSearch] = useState("");

  const [editOpen, setEditOpen] = useState(false);
  const [isUpdating, setIsUpdating] = useState(false);
  const [addContactOpen, setAddContactOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  const base = `${acct(accountId)}/companies/${companyId}`;

  const fetchCompany = useCallback(async () => {
    setIsLoading(true);
    setError("");
    try {
      const res = await api<{ payload: CompanyItem }>(base);
      setCompany(res.payload);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to load company");
    } finally {
      setIsLoading(false);
    }
  }, [base]);

  useEffect(() => {
    if (companyId) {
      fetchCompany();
    }
  }, [companyId, fetchCompany]);

  const handleSaveEdit = async (payload: Record<string, unknown>) => {
    setIsUpdating(true);
    try {
      const res = await api<{ payload: CompanyItem }>(base, {
        method: "PATCH",
        json: payload,
      });
      setCompany(res.payload);
    } finally {
      setIsUpdating(false);
    }
  };

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await api(base, { method: "DELETE" });
      navigate(`/accounts/${accountId}/companies`);
    } catch (err: unknown) {
      alert(err instanceof Error ? err.message : t("COMPANIES.DETAIL.DELETE.MESSAGES.ERROR"));
      setIsDeleting(false);
    }
  };

  const tabs = useMemo(
    () => [
      {
        key: "people",
        icon: "i-lucide-contact",
        label: t("COMPANIES.DETAIL.TABS.PEOPLE"),
        count: company?.contacts_count ?? 0,
      },
      {
        key: "conversations",
        icon: "i-lucide-message-circle",
        label: t("COMPANIES.DETAIL.TABS.CONVERSATIONS"),
      },
      {
        key: "notes",
        icon: "i-lucide-notebook-pen",
        label: t("COMPANIES.DETAIL.TABS.NOTES"),
      },
    ],
    [company?.contacts_count]
  );

  const activeTabIndex = tabs.findIndex((tab) => tab.key === activeTab);

  if (isLoading) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-n-surface-1">
        <Spinner size={32} />
      </div>
    );
  }

  if (error || !company) {
    return (
      <div className="flex flex-col h-full w-full items-center justify-center gap-3 bg-n-surface-1 p-6 text-center">
        <h2 className="text-lg font-medium text-n-slate-12">
          {t("COMPANIES.DETAIL.EMPTY_STATE.TITLE")}
        </h2>
        <p className="max-w-md text-sm text-n-slate-11">
          {error || t("COMPANIES.DETAIL.EMPTY_STATE.SUBTITLE")}
        </p>
        <Button
          variant="solid"
          color="blue"
          label={t("COMPANIES.HEADER")}
          onClick={() => navigate(`/accounts/${accountId}/companies`)}
        />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full w-full min-w-0 bg-n-surface-1 overflow-y-auto">
      <div className="w-full max-w-5xl mx-auto px-6 py-6 flex flex-col gap-8">
        {/* Back Breadcrumb */}
        <div>
          <Link
            to={`/accounts/${accountId}/companies`}
            className="inline-flex items-center gap-1.5 text-sm font-medium text-n-slate-11 hover:text-n-slate-12 transition-colors"
          >
            <Icon icon="i-lucide-chevron-left" className="size-4" />
            <span>{t("COMPANIES.HEADER")}</span>
          </Link>
        </div>

        {/* Company Header + Facts */}
        <div className="flex flex-col gap-6">
          <CompanyDetailHeader
            company={company}
            isAdmin={isAdmin}
            onEdit={() => setEditOpen(true)}
            onDelete={() => setDeleteOpen(true)}
          />
          <CompanyFacts company={company} />
        </div>

        {/* Tabs & Actions */}
        <div className="flex flex-col gap-4">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-n-weak pb-3">
            <TabBar
              tabs={tabs}
              initialActiveTab={activeTabIndex}
              onTabChanged={(tab) =>
                setActiveTab(tab.key as "people" | "conversations" | "notes")
              }
            />

            {activeTab === "people" && (
              <div className="flex items-center gap-2">
                <div className="relative w-56">
                  <Icon
                    icon="i-lucide-search"
                    className="absolute -translate-y-1/2 text-n-slate-11 size-3.5 top-1/2 ltr:left-2.5 rtl:right-2.5 pointer-events-none z-10"
                  />
                  <Input
                    modelValue={peopleSearch}
                    type="search"
                    placeholder={t("COMPANIES.DETAIL.PEOPLE.SEARCH")}
                    customInputClass="h-8 ltr:!pl-8 rtl:!pr-8 bg-n-alpha-2 dark:bg-n-solid-1 text-xs"
                    onModelValueChange={(v) => setPeopleSearch(String(v))}
                  />
                </div>
                <Button
                  label={t("COMPANIES.DETAIL.PEOPLE.ADD")}
                  icon="i-lucide-plus"
                  color="slate"
                  size="sm"
                  onClick={() => setAddContactOpen(true)}
                  className="shrink-0"
                />
              </div>
            )}
          </div>

          {/* Tab Panels */}
          <div className="pt-2">
            {activeTab === "people" && (
              <CompanyPeopleList
                accountId={accountId}
                companyId={company.id}
                query={peopleSearch}
                revision={peopleRevision}
                onContactsChanged={() => {
                  fetchCompany();
                }}
              />
            )}
            {activeTab === "conversations" && (
              <p className="px-4 py-10 text-sm text-center border border-dashed rounded-xl border-n-weak text-n-slate-11">
                {t("COMPANIES.DETAIL.ACTIVITY.EMPTY_CONVERSATIONS")}
              </p>
            )}
            {activeTab === "notes" && (
              <p className="px-4 py-10 text-sm text-center border border-dashed rounded-xl border-n-weak text-n-slate-11">
                {t("COMPANIES.DETAIL.ACTIVITY.EMPTY_NOTES")}
              </p>
            )}
          </div>
        </div>
      </div>

      <CompanyEditPanel
        open={editOpen}
        company={company}
        onClose={() => setEditOpen(false)}
        onSave={handleSaveEdit}
        isLoading={isUpdating}
      />

      <CompanyAddContactDialog
        accountId={accountId}
        company={company}
        open={addContactOpen}
        onClose={() => setAddContactOpen(false)}
        onLinked={() => {
          setPeopleRevision((n) => n + 1);
          fetchCompany();
        }}
      />

      <ConfirmCompanyDeleteDialog
        open={deleteOpen}
        company={company}
        onClose={() => setDeleteOpen(false)}
        onConfirm={handleDelete}
        isLoading={isDeleting}
      />
    </div>
  );
}
