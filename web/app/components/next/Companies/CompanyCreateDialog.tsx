import { useState } from "react";
import Button from "~/components/next/Button";
import Input from "~/components/next/Input";
import TextArea from "~/components/next/TextArea";
import { t } from "~/lib/i18n";

type Props = {
  open: boolean;
  onClose: () => void;
  onCreate: (payload: {
    name: string;
    domain: string | null;
    description: string | null;
  }) => Promise<void>;
  isLoading?: boolean;
};

export default function CompanyCreateDialog({
  open,
  onClose,
  onCreate,
  isLoading = false,
}: Props) {
  const [name, setName] = useState("");
  const [domain, setDomain] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState("");

  if (!open) return null;

  const reset = () => {
    setName("");
    setDomain("");
    setDescription("");
    setError("");
  };

  const handleClose = () => {
    reset();
    onClose();
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError("Company name is required");
      return;
    }

    setError("");
    try {
      await onCreate({
        name: name.trim(),
        domain: domain.trim() || null,
        description: description.trim() || null,
      });
      handleClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("COMPANIES.CREATE.MESSAGES.ERROR"));
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-2xl rounded-2xl border border-n-weak bg-n-surface-1 p-6 shadow-2xl flex flex-col gap-6">
        <div className="flex items-center justify-between border-b border-n-weak pb-4">
          <h2 className="text-lg font-medium text-n-slate-12">
            {t("COMPANIES.CREATE.TITLE")}
          </h2>
          <button
            type="button"
            onClick={handleClose}
            className="text-n-slate-10 hover:text-n-slate-12 transition-colors p-1"
            aria-label="Close"
          >
            <span className="text-xl leading-none">&times;</span>
          </button>
        </div>

        {error && (
          <div className="rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-sm text-n-ruby-11">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="flex flex-col gap-5">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <Input
              label={t("COMPANIES.DETAIL.PROFILE.FIELDS.NAME")}
              placeholder="Acme Inc."
              modelValue={name}
              onModelValueChange={(v) => setName(String(v))}
              required
              autofocus
            />
            <Input
              label={t("COMPANIES.DETAIL.PROFILE.FIELDS.DOMAIN")}
              placeholder="acme.com"
              modelValue={domain}
              onModelValueChange={(v) => setDomain(String(v))}
            />
          </div>

          <TextArea
            label={t("COMPANIES.DETAIL.EDIT.DESCRIPTION")}
            placeholder={t("COMPANIES.DETAIL.PROFILE.DESCRIPTION_PLACEHOLDER")}
            modelValue={description}
            onModelValueChange={(v) => setDescription(v)}
            maxLength={280}
            showCharacterCount
            autoHeight
          />

          <div className="flex items-center justify-between pt-4 border-t border-n-weak">
            <Button
              type="button"
              variant="link"
              color="slate"
              label={t("DIALOG.BUTTONS.CANCEL")}
              onClick={handleClose}
              disabled={isLoading}
            />
            <Button
              type="submit"
              variant="solid"
              color="blue"
              label={t("COMPANIES.CREATE.ACTIONS.SAVE")}
              isLoading={isLoading}
              disabled={isLoading || !name.trim()}
            />
          </div>
        </form>
      </div>
    </div>
  );
}
