import Button from "~/components/next/Button";
import Input from "~/components/next/Input";
import Icon from "~/components/next/Icon";
import ContactSortMenu from "./ContactSortMenu";
import ContactFilter, { type FilterCondition } from "./ContactFilter";
import { t } from "~/lib/i18n";

type Props = {
  headerTitle: string;
  searchValue: string;
  onSearch: (value: string) => void;
  activeSort: string;
  activeOrdering: string;
  onSortChange: (value: { sort: string; order: string }) => void;
  hasActiveFilters: boolean;
  onApplyFilters: (filters: FilterCondition[]) => void;
  onClearFilters: () => void;
  onNewContact: () => void;
};

export default function ContactHeader({
  headerTitle,
  searchValue,
  onSearch,
  activeSort,
  activeOrdering,
  onSortChange,
  hasActiveFilters,
  onApplyFilters,
  onClearFilters,
  onNewContact,
}: Props) {
  return (
    <header className="sticky top-0 z-20 px-6 bg-n-surface-1/80 backdrop-blur-md border-b border-n-weak">
      <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between w-full py-4 gap-3 mx-auto max-w-5xl">
        <h1 className="text-xl font-medium truncate text-n-slate-12">
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
              placeholder={t("CONTACTS_LAYOUT.HEADER.SEARCH_PLACEHOLDER")}
              customInputClass="h-8 ltr:!pl-8 rtl:!pr-8 bg-n-alpha-2 dark:bg-n-solid-1 text-xs"
              onModelValueChange={(v) => onSearch(String(v))}
            />
          </div>

          <div className="flex items-center gap-1.5 shrink-0">
            <ContactFilter
              hasActiveFilters={hasActiveFilters}
              onApplyFilters={onApplyFilters}
              onClearFilters={onClearFilters}
            />
            <ContactSortMenu
              activeSort={activeSort}
              activeOrdering={activeOrdering}
              onSortChange={onSortChange}
            />
          </div>

          <div className="w-px h-4 bg-n-strong hidden sm:block" />

          <Button
            variant="solid"
            color="blue"
            size="sm"
            icon="i-lucide-plus"
            label={t("CONTACTS_LAYOUT.HEADER.ACTIONS.CONTACT_CREATION.ADD_CONTACT")}
            onClick={onNewContact}
            className="shrink-0"
          />
        </div>
      </div>
    </header>
  );
}
