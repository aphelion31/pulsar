import Button from "~/components/next/Button";
import { t } from "~/lib/i18n";

type Props = {
  currentPage: number;
  totalItems: number;
  itemsPerPage?: number;
  onPageChange: (page: number) => void;
  className?: string;
};

export default function CompanyPaginationFooter({
  currentPage,
  totalItems,
  itemsPerPage = 50,
  onPageChange,
  className = "",
}: Props) {
  const totalPages = Math.max(1, Math.ceil(totalItems / itemsPerPage));
  const startItem = totalItems === 0 ? 0 : (currentPage - 1) * itemsPerPage + 1;
  const endItem = Math.min(startItem + itemsPerPage - 1, totalItems);

  const isFirstPage = currentPage <= 1;
  const isLastPage = currentPage >= totalPages;

  if (totalItems <= 0) return null;

  const infoText = `Showing ${startItem} – ${endItem} of ${totalItems} ${
    totalItems === 1 ? "company" : "companies"
  }`;

  const pageOfText = t("PAGINATION_FOOTER.CURRENT_PAGE_INFO", {
    currentPage: "",
    totalPages,
    count: totalPages,
  });

  return (
    <div
      className={`flex items-center justify-between h-[3.375rem] w-full border-t border-n-weak bg-n-surface-1 py-3 px-6 select-none ${className}`}
    >
      <div className="flex items-center gap-3 min-w-0">
        <span className="text-sm truncate text-n-slate-11">{infoText}</span>
      </div>
      <div className="flex items-center gap-2 shrink-0">
        <Button
          icon="i-lucide-chevrons-left"
          variant="ghost"
          color="slate"
          size="sm"
          className="!w-8 !h-8 p-0"
          disabled={isFirstPage}
          onClick={() => onPageChange(1)}
          aria-label="First page"
        />
        <Button
          icon="i-lucide-chevron-left"
          variant="ghost"
          color="slate"
          size="sm"
          className="!w-8 !h-8 p-0"
          disabled={isFirstPage}
          onClick={() => onPageChange(currentPage - 1)}
          aria-label="Previous page"
        />
        <div className="inline-flex items-center gap-1.5 text-sm">
          <span className="px-2.5 py-0.5 tabular-nums font-medium bg-n-alpha-black2 text-n-slate-12 rounded-md">
            {currentPage}
          </span>
          <span className="text-n-slate-11">{pageOfText}</span>
        </div>
        <Button
          icon="i-lucide-chevron-right"
          variant="ghost"
          color="slate"
          size="sm"
          className="!w-8 !h-8 p-0"
          disabled={isLastPage}
          onClick={() => onPageChange(currentPage + 1)}
          aria-label="Next page"
        />
        <Button
          icon="i-lucide-chevrons-right"
          variant="ghost"
          color="slate"
          size="sm"
          className="!w-8 !h-8 p-0"
          disabled={isLastPage}
          onClick={() => onPageChange(totalPages)}
          aria-label="Last page"
        />
      </div>
    </div>
  );
}
