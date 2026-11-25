import { useState, useEffect } from "react";
import Avatar from "~/components/next/Avatar";
import Button from "~/components/next/Button";
import Input from "~/components/next/Input";
import Spinner from "~/components/next/Spinner";
import { api, acct } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { CompanyItem, CompanyContactItem } from "./types";

type Props = {
  accountId: number;
  company: CompanyItem;
  open: boolean;
  onClose: () => void;
  onLinked: () => void;
};

export default function CompanyAddContactDialog({
  accountId,
  company,
  open,
  onClose,
  onLinked,
}: Props) {
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [results, setResults] = useState<CompanyContactItem[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [selectedContact, setSelectedContact] = useState<CompanyContactItem | null>(null);
  const [isLinking, setIsLinking] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedSearch(search), 300);
    return () => clearTimeout(timer);
  }, [search]);

  useEffect(() => {
    if (!open || !debouncedSearch.trim()) {
      setResults([]);
      setIsSearching(false);
      return;
    }

    let active = true;
    setIsSearching(true);
    setError("");
    api<{ payload: CompanyContactItem[] }>(
      `${acct(accountId)}/companies/${company.id}/contacts/search?q=${encodeURIComponent(
        debouncedSearch.trim()
      )}`
    )
      .then((res) => {
        if (active) setResults(res.payload ?? []);
      })
      .catch((err: unknown) => {
        if (active) setError(err instanceof Error ? err.message : "Search failed");
      })
      .finally(() => {
        if (active) setIsSearching(false);
      });

    return () => {
      active = false;
    };
  }, [open, accountId, company.id, debouncedSearch]);

  if (!open) return null;

  const handleConfirm = async () => {
    if (!selectedContact) return;
    setIsLinking(true);
    setError("");

    try {
      await api(`${acct(accountId)}/companies/${company.id}/contacts`, {
        method: "POST",
        json: { contact_id: selectedContact.id },
      });
      setSelectedContact(null);
      setSearch("");
      onLinked();
      onClose();
    } catch (err: unknown) {
      setError(
        err instanceof Error
          ? err.message
          : t("COMPANIES.DETAIL.CONTACTS.MESSAGES.ADD_ERROR")
      );
    } finally {
      setIsLinking(false);
    }
  };

  const handleClose = () => {
    setSelectedContact(null);
    setSearch("");
    setError("");
    onClose();
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-lg rounded-2xl border border-n-weak bg-n-surface-1 p-6 shadow-2xl flex flex-col gap-5">
        <div className="flex items-center justify-between border-b border-n-weak pb-3">
          <h3 className="text-base font-medium text-n-slate-12">
            {t("COMPANIES.DETAIL.PEOPLE.DIALOG_TITLE", { name: company.name })}
          </h3>
          <button
            type="button"
            onClick={handleClose}
            className="text-n-slate-10 hover:text-n-slate-12 p-1"
            aria-label="Close"
          >
            <span className="text-xl leading-none">&times;</span>
          </button>
        </div>

        {error && (
          <div className="rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-xs text-n-ruby-11">
            {error}
          </div>
        )}

        {!selectedContact ? (
          <div className="flex flex-col gap-3">
            <p className="text-sm text-n-slate-11">
              {t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.DESCRIPTION")}
            </p>
            <Input
              placeholder={t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.SEARCH_PLACEHOLDER")}
              modelValue={search}
              onModelValueChange={(v) => setSearch(String(v))}
              autofocus
            />

            {isSearching ? (
              <div className="flex items-center justify-center py-6 text-n-slate-11">
                <Spinner size={20} />
              </div>
            ) : results.length > 0 ? (
              <div className="max-h-60 overflow-y-auto rounded-xl border border-n-weak divide-y divide-n-weak bg-n-surface-1">
                {results.map((c) => (
                  <button
                    key={c.id}
                    type="button"
                    onClick={() => setSelectedContact(c)}
                    className="w-full flex items-center gap-3 p-3 text-left hover:bg-n-solid-2 transition-colors"
                  >
                    <Avatar
                      name={c.name || t("COMPANIES.DETAIL.CONTACTS.UNNAMED_CONTACT")}
                      src={c.thumbnail || ""}
                      size={32}
                      hideOfflineStatus
                    />
                    <div className="flex flex-col min-w-0 flex-1">
                      <span className="text-sm font-medium truncate text-n-slate-12">
                        {c.name || t("COMPANIES.DETAIL.CONTACTS.UNNAMED_CONTACT")}
                      </span>
                      <span className="text-xs text-n-slate-11 truncate">
                        {[c.email, c.phone_number].filter(Boolean).join(" • ") || `ID: ${c.id}`}
                      </span>
                    </div>
                  </button>
                ))}
              </div>
            ) : debouncedSearch.trim() ? (
              <p className="text-xs text-center py-4 text-n-slate-10">
                {t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.EMPTY")}
              </p>
            ) : (
              <p className="text-xs text-center py-4 text-n-slate-10">
                {t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.INITIAL")}
              </p>
            )}
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-1">
              <span className="text-sm font-medium text-n-slate-12">
                {t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.CONFIRM_TITLE")}
              </span>
              <span className="text-xs text-n-slate-11">
                {t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.CONFIRM_DESCRIPTION")}
              </span>
            </div>

            <div className="flex flex-col gap-3">
              <div className="flex flex-col gap-1">
                <span className="text-xs font-medium text-n-slate-11">
                  {t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.COMPANY_LABEL")}
                </span>
                <div className="flex items-center gap-3 p-3 rounded-xl border border-n-strong bg-n-solid-2">
                  <Avatar name={company.name} src={company.avatar_url || ""} size={32} hideOfflineStatus />
                  <div className="flex flex-col min-w-0">
                    <span className="text-sm font-medium truncate text-n-slate-12">{company.name}</span>
                    {company.domain && (
                      <span className="text-xs text-n-slate-11 truncate">{company.domain}</span>
                    )}
                  </div>
                </div>
              </div>

              <div className="flex flex-col gap-1">
                <span className="text-xs font-medium text-n-slate-11">
                  {t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.CONTACT_LABEL")}
                </span>
                <div className="flex items-center gap-3 p-3 rounded-xl border border-n-strong bg-n-solid-2">
                  <Avatar
                    name={selectedContact.name || t("COMPANIES.DETAIL.CONTACTS.UNNAMED_CONTACT")}
                    src={selectedContact.thumbnail || ""}
                    size={32}
                    hideOfflineStatus
                  />
                  <div className="flex flex-col min-w-0">
                    <span className="text-sm font-medium truncate text-n-slate-12">
                      {selectedContact.name || t("COMPANIES.DETAIL.CONTACTS.UNNAMED_CONTACT")}
                    </span>
                    <span className="text-xs text-n-slate-11 truncate">
                      {[selectedContact.email, selectedContact.phone_number]
                        .filter(Boolean)
                        .join(" • ") || "—"}
                    </span>
                  </div>
                </div>
              </div>
            </div>

            <div className="flex items-center justify-end gap-3 pt-4 border-t border-n-weak">
              <Button
                type="button"
                variant="faded"
                color="slate"
                label={t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.CANCEL")}
                onClick={() => setSelectedContact(null)}
                disabled={isLinking}
              />
              <Button
                type="button"
                variant="solid"
                color="blue"
                label={t("COMPANIES.DETAIL.CONTACTS.DIALOGS.ADD.ADD")}
                isLoading={isLinking}
                disabled={isLinking}
                onClick={handleConfirm}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
