import { useState, useRef, useEffect } from "react";
import Button from "~/components/next/Button";
import { t } from "~/lib/i18n";

type Props = {
  activeSort: string;
  activeOrdering: string;
  onSortChange: (value: { sort: string; order: string }) => void;
};

const SORT_OPTIONS = [
  { value: "last_activity_at", labelKey: "CONTACTS_LAYOUT.HEADER.ACTIONS.SORT_BY.OPTIONS.LAST_ACTIVITY" },
  { value: "name", labelKey: "CONTACTS_LAYOUT.HEADER.ACTIONS.SORT_BY.OPTIONS.NAME" },
  { value: "email", labelKey: "CONTACTS_LAYOUT.HEADER.ACTIONS.SORT_BY.OPTIONS.EMAIL" },
  { value: "phone_number", labelKey: "CONTACTS_LAYOUT.HEADER.ACTIONS.SORT_BY.OPTIONS.PHONE" },
  { value: "created_at", labelKey: "CONTACTS_LAYOUT.HEADER.ACTIONS.SORT_BY.OPTIONS.CREATED_AT" },
];

const ORDER_OPTIONS = [
  { value: "", labelKey: "CONTACTS_LAYOUT.HEADER.ACTIONS.ORDER.OPTIONS.ASCENDING" },
  { value: "-", labelKey: "CONTACTS_LAYOUT.HEADER.ACTIONS.ORDER.OPTIONS.DESCENDING" },
];

export default function ContactSortMenu({
  activeSort,
  activeOrdering,
  onSortChange,
}: Props) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [open]);

  return (
    <div ref={menuRef} className="relative">
      <Button
        icon="i-lucide-arrow-down-up"
        color="slate"
        size="sm"
        variant="ghost"
        className={open ? "bg-n-alpha-2" : ""}
        onClick={() => setOpen(!open)}
        aria-label="Sort contacts"
      />
      {open && (
        <div className="absolute top-full mt-1.5 ltr:right-0 rtl:left-0 flex flex-col gap-4 bg-n-alpha-3 backdrop-blur-[100px] border border-n-weak w-72 rounded-xl p-4 z-50 shadow-lg">
          <div className="flex items-center justify-between gap-2">
            <span className="text-sm font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.HEADER.ACTIONS.SORT_BY.LABEL")}
            </span>
            <select
              value={activeSort}
              onChange={(e) => {
                onSortChange({ sort: e.target.value, order: activeOrdering });
              }}
              className="text-xs rounded-lg border border-n-strong bg-n-surface-1 px-2.5 py-1.5 text-n-slate-12 outline-none focus:border-n-brand"
            >
              {SORT_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-sm font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.HEADER.ACTIONS.ORDER.LABEL")}
            </span>
            <select
              value={activeOrdering}
              onChange={(e) => {
                onSortChange({ sort: activeSort, order: e.target.value });
              }}
              className="text-xs rounded-lg border border-n-strong bg-n-surface-1 px-2.5 py-1.5 text-n-slate-12 outline-none focus:border-n-brand"
            >
              {ORDER_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
        </div>
      )}
    </div>
  );
}
