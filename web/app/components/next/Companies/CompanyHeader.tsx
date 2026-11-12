import { useState, useRef, useEffect } from "react";
import Button from "~/components/next/Button";
import Input from "~/components/next/Input";
import Icon from "~/components/next/Icon";
import { t } from "~/lib/i18n";

type Props = {
  headerTitle: string;
  searchValue: string;
  onSearch: (val: string) => void;
  activeSort: string;
  activeOrdering: string;
  onSortChange: (val: { sort: string; order: string }) => void;
  onCreate: () => void;
};

const SORT_OPTIONS = [
  { value: "name", labelKey: "COMPANIES.SORT_BY.OPTIONS.NAME" },
  { value: "domain", labelKey: "COMPANIES.SORT_BY.OPTIONS.DOMAIN" },
  { value: "created_at", labelKey: "COMPANIES.SORT_BY.OPTIONS.CREATED_AT" },
  { value: "last_activity_at", labelKey: "COMPANIES.SORT_BY.OPTIONS.LAST_ACTIVITY_AT" },
  { value: "contacts_count", labelKey: "COMPANIES.SORT_BY.OPTIONS.CONTACTS_COUNT" },
];

const ORDER_OPTIONS = [
  { value: "", labelKey: "COMPANIES.ORDER.OPTIONS.ASCENDING" },
  { value: "-", labelKey: "COMPANIES.ORDER.OPTIONS.DESCENDING" },
];

export default function CompanyHeader({
  headerTitle,
  searchValue,
  onSearch,
  activeSort,
  activeOrdering,
  onSortChange,
  onCreate,
}: Props) {
  const [sortOpen, setSortOpen] = useState(false);
  const sortRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (sortRef.current && !sortRef.current.contains(e.target as Node)) {
        setSortOpen(false);
      }
    }
    if (sortOpen) document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [sortOpen]);

  return (
    <header className="sticky top-0 z-20 px-6 bg-n-surface-1/80 backdrop-blur-md border-b border-n-weak">
      <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between w-full py-4 gap-3 mx-auto max-w-5xl">
        <h1 className="text-xl font-medium shrink-0 text-n-slate-12">
          {headerTitle}
        </h1>

        <div className="flex items-center flex-wrap sm:flex-nowrap gap-3 w-full sm:w-auto">
          <div className="relative flex-1 sm:w-64 min-w-40">
            <Icon
              icon="i-lucide-search"
              className="absolute -translate-y-1/2 text-n-slate-11 size-4 top-1/2 ltr:left-2.5 rtl:right-2.5 pointer-events-none z-10"
            />
            <Input
              modelValue={searchValue}
              type="search"
              placeholder={t("COMPANIES.SEARCH_PLACEHOLDER")}
              customInputClass="h-8 ltr:!pl-8 rtl:!pr-8 bg-n-alpha-2 dark:bg-n-solid-1 text-xs"
              onModelValueChange={(v) => onSearch(String(v))}
            />
          </div>

          <div ref={sortRef} className="relative shrink-0">
            <Button
              icon="i-lucide-arrow-down-up"
              color="slate"
              size="sm"
              variant="ghost"
              className={sortOpen ? "bg-n-alpha-2" : ""}
              onClick={() => setSortOpen(!sortOpen)}
              aria-label="Sort companies"
            />
            {sortOpen && (
              <div className="absolute top-full mt-1.5 ltr:right-0 rtl:left-0 flex flex-col gap-4 bg-n-alpha-3 backdrop-blur-[100px] border border-n-weak w-72 rounded-xl p-4 z-50 shadow-lg">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-sm font-medium text-n-slate-12">
                    {t("COMPANIES.SORT_BY.LABEL")}
                  </span>
                  <select
                    value={activeSort}
                    onChange={(e) =>
                      onSortChange({ sort: e.target.value, order: activeOrdering })
                    }
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
                    {t("COMPANIES.ORDER.LABEL")}
                  </span>
                  <select
                    value={activeOrdering}
                    onChange={(e) =>
                      onSortChange({ sort: activeSort, order: e.target.value })
                    }
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

          <div className="w-px h-4 bg-n-strong hidden sm:block" />

          <Button
            variant="solid"
            color="blue"
            size="sm"
            icon="i-lucide-plus"
            label={t("COMPANIES.ACTIONS.CREATE")}
            onClick={onCreate}
            className="shrink-0"
          />
        </div>
      </div>
    </header>
  );
}
