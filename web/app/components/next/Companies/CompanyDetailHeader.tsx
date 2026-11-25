import { useState } from "react";
import Avatar from "~/components/next/Avatar";
import Button from "~/components/next/Button";
import Icon from "~/components/next/Icon";
import { t } from "~/lib/i18n";
import type { CompanyItem } from "./types";

type Props = {
  company: CompanyItem;
  isAdmin: boolean;
  onEdit: () => void;
  onDelete: () => void;
};

export default function CompanyDetailHeader({
  company,
  isAdmin,
  onEdit,
  onDelete,
}: Props) {
  const [expanded, setExpanded] = useState(false);

  const displayName = company.name || t("COMPANIES.UNNAMED");
  const attrs = (company.additional_attributes ?? {}) as Record<string, any>;
  const phone = attrs.phone;
  const socialProfiles = (attrs.social_profiles ?? {}) as Record<string, string>;
  const isLongDesc = (company.description || "").length > 200;

  return (
    <header className="flex flex-col gap-4">
      <div className="flex items-start gap-4">
        <Avatar
          name={displayName}
          src={company.avatar_url || ""}
          size={48}
          hideOfflineStatus
        />

        <div className="flex flex-col flex-1 min-w-0 gap-1.5">
          <h1 className="text-xl font-medium break-words text-n-slate-12">
            {displayName}
          </h1>

          <div className="flex flex-wrap items-center text-sm gap-x-3 gap-y-1 text-n-slate-11">
            {company.domain && (
              <a
                href={`https://${company.domain}`}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 hover:text-n-slate-12 transition-colors"
              >
                {company.domain}
                <Icon icon="i-lucide-arrow-up-right" className="size-3.5" />
              </a>
            )}
            {phone && (
              <a
                href={`tel:${phone}`}
                className="inline-flex items-center gap-1 hover:text-n-slate-12 transition-colors"
              >
                <Icon icon="i-lucide-phone" className="size-3.5" />
                <span>{phone}</span>
              </a>
            )}
            {Object.entries(socialProfiles).map(([key, url]) =>
              url ? (
                <a
                  key={key}
                  href={url.startsWith("http") ? url : `https://${url}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-xs capitalize text-n-slate-10 hover:text-n-brand transition-colors"
                >
                  {key}
                </a>
              ) : null
            )}
          </div>
        </div>

        <div className="flex items-center gap-1.5 shrink-0">
          <Button
            icon="i-lucide-pencil"
            variant="ghost"
            color="slate"
            size="sm"
            onClick={onEdit}
            aria-label={t("COMPANIES.DETAIL.HEADER.EDIT")}
          />
          {isAdmin && (
            <Button
              icon="i-lucide-trash-2"
              variant="ghost"
              color="ruby"
              size="sm"
              onClick={onDelete}
              aria-label={t("COMPANIES.DETAIL.DELETE.BUTTON")}
            />
          )}
        </div>
      </div>

      {company.description && (
        <div className="flex flex-col items-start max-w-4xl gap-1">
          <p
            className={`mb-0 text-sm text-n-slate-11 ${
              expanded ? "" : "line-clamp-2"
            }`}
          >
            {company.description}
          </p>
          {isLongDesc && (
            <button
              type="button"
              className="p-0 text-xs text-n-slate-11 hover:text-n-slate-12"
              onClick={() => setExpanded(!expanded)}
            >
              {expanded
                ? t("COMPANIES.DETAIL.HEADER.LESS")
                : t("COMPANIES.DETAIL.HEADER.MORE")}
            </button>
          )}
        </div>
      )}
    </header>
  );
}
