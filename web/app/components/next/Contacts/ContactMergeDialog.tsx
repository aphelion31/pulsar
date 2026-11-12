import { useState, useEffect } from "react";
import Button from "~/components/next/Button";
import Avatar from "~/components/next/Avatar";
import Input from "~/components/next/Input";
import Spinner from "~/components/next/Spinner";
import { api, acct } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ContactItem } from "./types";

type Props = {
  accountId: number;
  contact: ContactItem;
  open: boolean;
  onClose: () => void;
  onMerged: (baseContactId: number) => void;
};

export default function ContactMergeDialog({
  accountId,
  contact,
  open,
  onClose,
  onMerged,
}: Props) {
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [results, setResults] = useState<ContactItem[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [selectedParent, setSelectedParent] = useState<ContactItem | null>(null);
  const [isMerging, setIsMerging] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(search);
    }, 300);
    return () => clearTimeout(timer);
  }, [search]);

  useEffect(() => {
    if (!debouncedSearch.trim()) {
      setResults([]);
      setIsSearching(false);
      return;
    }

    let active = true;
    setIsSearching(true);
    api<{ payload: ContactItem[] }>(
      `${acct(accountId)}/contacts/search?q=${encodeURIComponent(debouncedSearch.trim())}`
    )
      .then((res) => {
        if (!active) return;
        const filtered = (res.payload ?? []).filter((c) => c.id !== contact.id);
        setResults(filtered);
      })
      .catch((err: unknown) => {
        if (!active) return;
        setError(err instanceof Error ? err.message : "Search failed");
      })
      .finally(() => {
        if (active) setIsSearching(false);
      });

    return () => {
      active = false;
    };
  }, [accountId, debouncedSearch, contact.id]);

  if (!open) return null;

  const handleMerge = async () => {
    if (!selectedParent) return;

    setIsMerging(true);
    setError("");

    try {
      await api(`${acct(accountId)}/actions/contact_merge`, {
        method: "POST",
        json: {
          base_contact_id: selectedParent.id,
          mergee_contact_id: contact.id,
        },
      });
      onMerged(selectedParent.id);
    } catch (err: unknown) {
      setError(
        err instanceof Error
          ? err.message
          : t("CONTACTS_LAYOUT.SIDEBAR.MERGE.ERROR_MESSAGE")
      );
    } finally {
      setIsMerging(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-lg max-h-[90vh] overflow-y-auto rounded-2xl border border-n-weak bg-n-surface-1 p-6 shadow-2xl flex flex-col gap-5">
        <div className="flex flex-col gap-1 border-b border-n-weak pb-3">
          <h3 className="text-base font-medium text-n-slate-12">
            {t("CONTACTS_LAYOUT.SIDEBAR.MERGE.TITLE")}
          </h3>
          <p className="text-xs text-n-slate-11">
            {t("CONTACTS_LAYOUT.SIDEBAR.MERGE.DESCRIPTION")}
          </p>
        </div>

        {error && (
          <div className="rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-xs text-n-ruby-11">
            {error}
          </div>
        )}

        {/* Selected Primary / Target Contact */}
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.SIDEBAR.MERGE.PRIMARY")}
            </span>
            <span className="text-2xs px-2 py-0.5 rounded bg-n-teal-9/10 text-n-teal-11 font-medium">
              {t("CONTACTS_LAYOUT.SIDEBAR.MERGE.PRIMARY_HELP_LABEL")}
            </span>
          </div>

          {selectedParent ? (
            <div className="flex items-center justify-between p-3 rounded-xl border border-n-strong bg-n-solid-2">
              <div className="flex items-center gap-3 min-w-0">
                <Avatar
                  name={selectedParent.name || "Target"}
                  src={selectedParent.thumbnail || ""}
                  size={32}
                  hideOfflineStatus
                />
                <div className="flex flex-col min-w-0">
                  <span className="text-sm font-medium truncate text-n-slate-12">
                    {selectedParent.name || "Unknown"}
                  </span>
                  <span className="text-xs text-n-slate-11 truncate">
                    {selectedParent.email || selectedParent.phone_number || "—"}
                  </span>
                </div>
              </div>
              <Button
                variant="ghost"
                color="slate"
                size="xs"
                label="Change"
                onClick={() => setSelectedParent(null)}
              />
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <Input
                placeholder={t("CONTACTS_LAYOUT.SIDEBAR.MERGE.SEARCH_PLACEHOLDER")}
                modelValue={search}
                onModelValueChange={(v) => setSearch(String(v))}
              />

              {isSearching ? (
                <div className="flex items-center justify-center py-4 text-n-slate-11">
                  <Spinner size={20} />
                </div>
              ) : results.length > 0 ? (
                <div className="max-h-48 overflow-y-auto rounded-xl border border-n-weak divide-y divide-n-weak bg-n-surface-1">
                  {results.map((c) => (
                    <button
                      key={c.id}
                      type="button"
                      onClick={() => setSelectedParent(c)}
                      className="w-full flex items-center gap-3 p-2.5 text-left hover:bg-n-solid-2 transition-colors"
                    >
                      <Avatar
                        name={c.name || "Contact"}
                        src={c.thumbnail || ""}
                        size={28}
                        hideOfflineStatus
                      />
                      <div className="flex flex-col min-w-0 flex-1">
                        <span className="text-xs font-medium truncate text-n-slate-12">
                          {c.name || "Unnamed contact"}
                        </span>
                        <span className="text-2xs text-n-slate-11 truncate">
                          {c.email || c.phone_number || `ID: ${c.id}`}
                        </span>
                      </div>
                    </button>
                  ))}
                </div>
              ) : debouncedSearch ? (
                <p className="text-xs text-center py-2 text-n-slate-10">
                  {t("CONTACTS_LAYOUT.SIDEBAR.MERGE.EMPTY_STATE")}
                </p>
              ) : null}
            </div>
          )}
        </div>

        {/* Arrow divider */}
        <div className="flex justify-center text-n-slate-9 py-1">
          <span className="text-xs uppercase tracking-wider font-semibold text-n-slate-10">
            &darr; absorbs &darr;
          </span>
        </div>

        {/* Current Mergee Contact */}
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.SIDEBAR.MERGE.PARENT")}
            </span>
            <span className="text-2xs px-2 py-0.5 rounded bg-n-ruby-9/10 text-n-ruby-11 font-medium">
              {t("CONTACTS_LAYOUT.SIDEBAR.MERGE.PARENT_HELP_LABEL")}
            </span>
          </div>
          <div className="flex items-center gap-3 p-3 rounded-xl border border-n-weak bg-n-solid-2 opacity-80">
            <Avatar
              name={contact.name || "Current"}
              src={contact.thumbnail || ""}
              size={32}
              hideOfflineStatus
            />
            <div className="flex flex-col min-w-0">
              <span className="text-sm font-medium truncate text-n-slate-12">
                {contact.name || "Current contact"}
              </span>
              <span className="text-xs text-n-slate-11 truncate">
                {contact.email || contact.phone_number || `ID: ${contact.id}`}
              </span>
            </div>
          </div>
        </div>

        {/* Actions */}
        <div className="flex items-center justify-end gap-3 pt-4 border-t border-n-weak">
          <Button
            type="button"
            variant="faded"
            color="slate"
            label={t("CONTACTS_LAYOUT.SIDEBAR.MERGE.BUTTONS.CANCEL")}
            onClick={onClose}
            disabled={isMerging}
          />
          <Button
            type="button"
            variant="solid"
            color="blue"
            label={t("CONTACTS_LAYOUT.SIDEBAR.MERGE.BUTTONS.CONFIRM")}
            isLoading={isMerging}
            disabled={isMerging || !selectedParent}
            onClick={handleMerge}
          />
        </div>
      </div>
    </div>
  );
}
