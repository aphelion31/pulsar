import Button from "~/components/next/Button";
import { t } from "~/lib/i18n";
import type { Contact } from "~/lib/api";

type Props = {
  open: boolean;
  contact: Contact | null;
  onClose: () => void;
  onConfirm: () => Promise<void>;
  isLoading?: boolean;
};

export default function ConfirmContactDeleteDialog({
  open,
  contact,
  onClose,
  onConfirm,
  isLoading = false,
}: Props) {
  if (!open || !contact) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-md rounded-2xl border border-n-weak bg-n-surface-1 p-6 shadow-2xl flex flex-col gap-4">
        <h3 className="text-base font-medium text-n-slate-12">
          {t("CONTACTS_LAYOUT.DETAILS.DELETE_CONTACT")}
        </h3>
        <p className="text-sm text-n-slate-11">
          {t("CONTACTS_LAYOUT.DETAILS.DELETE_CONTACT_DESCRIPTION")}
        </p>
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
            label={t("CONTACTS_LAYOUT.DETAILS.DELETE_CONTACT")}
            isLoading={isLoading}
            disabled={isLoading}
            onClick={onConfirm}
          />
        </div>
      </div>
    </div>
  );
}
