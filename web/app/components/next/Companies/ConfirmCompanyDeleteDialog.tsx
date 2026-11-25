import Button from "~/components/next/Button";
import { t } from "~/lib/i18n";
import type { CompanyItem } from "./types";

type Props = {
  open: boolean;
  company: CompanyItem | null;
  onClose: () => void;
  onConfirm: () => Promise<void>;
  isLoading?: boolean;
};

export default function ConfirmCompanyDeleteDialog({
  open,
  company,
  onClose,
  onConfirm,
  isLoading = false,
}: Props) {
  if (!open || !company) return null;

  const description = company.name
    ? t("COMPANIES.DETAIL.DELETE.DESCRIPTION_WITH_NAME", {
        companyName: company.name,
      })
    : t("COMPANIES.DETAIL.DELETE.DESCRIPTION");

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-md rounded-2xl border border-n-weak bg-n-surface-1 p-6 shadow-2xl flex flex-col gap-4">
        <h3 className="text-base font-medium text-n-slate-12">
          {t("COMPANIES.DETAIL.DELETE.TITLE")}
        </h3>
        <p className="text-sm text-n-slate-11">{description}</p>
        <div className="flex items-center justify-end gap-3 pt-4 border-t border-n-weak">
          <Button
            type="button"
            variant="faded"
            color="slate"
            label={t("DIALOG.BUTTONS.CANCEL")}
            onClick={onClose}
            disabled={isLoading}
          />
          <Button
            type="button"
            variant="solid"
            color="ruby"
            label={t("COMPANIES.DETAIL.DELETE.CONFIRM")}
            isLoading={isLoading}
            disabled={isLoading}
            onClick={onConfirm}
          />
        </div>
      </div>
    </div>
  );
}
