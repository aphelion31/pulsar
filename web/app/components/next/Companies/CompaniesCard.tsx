import Avatar from "~/components/next/Avatar";
import Icon from "~/components/next/Icon";
import { timeAgo } from "~/lib/format";
import { t } from "~/lib/i18n";
import type { CompanyItem } from "./types";

type Props = {
  company: CompanyItem;
  onShowCompany: (id: number) => void;
};

export default function CompaniesCard({ company, onShowCompany }: Props) {
  const displayName = company.name || t("COMPANIES.UNNAMED");
  const attrs = (company.additional_attributes ?? {}) as Record<string, any>;
  const industry = attrs.sub_industry || attrs.industry;
  const employees = attrs.employee_count
    ? Number(attrs.employee_count).toLocaleString()
    : attrs.employee_count_range;
  const location = [attrs.city, attrs.country || attrs.country_code]
    .filter(Boolean)
    .join(", ");
  const contactsCount = Number(company.contacts_count || 0);

  const details: Array<{ key: string; icon: string; label: string }> = [];
  if (company.domain) {
    details.push({ key: "domain", icon: "i-lucide-globe", label: company.domain });
  }
  if (industry) {
    details.push({ key: "industry", icon: "i-lucide-building-2", label: industry });
  }
  if (employees) {
    details.push({
      key: "employees",
      icon: "i-lucide-users",
      label: `${employees} employees`,
    });
  }
  if (location) {
    details.push({ key: "location", icon: "i-lucide-map-pin", label: location });
  }
  if (contactsCount > 0) {
    details.push({
      key: "contacts",
      icon: "i-lucide-contact",
      label: `${contactsCount} ${contactsCount === 1 ? "contact" : "contacts"}`,
    });
  }

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => onShowCompany(company.id)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onShowCompany(company.id);
        }
      }}
      className="flex items-start justify-between gap-4 py-4 px-2 rounded-xl cursor-pointer group hover:bg-n-solid-2 transition-colors"
    >
      <div className="flex items-start flex-1 min-w-0 gap-3.5">
        <Avatar
          name={displayName}
          src={company.avatar_url || ""}
          size={36}
          hideOfflineStatus
        />
        <div className="flex flex-col flex-1 min-w-0 gap-1">
          <span className="block truncate text-sm font-medium text-n-slate-12 group-hover:text-n-blue-11 transition-colors">
            {displayName}
          </span>
          {company.description && (
            <p className="mb-0 text-n-slate-11 text-xs line-clamp-1">
              {company.description}
            </p>
          )}
          {details.length > 0 && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 mt-0.5">
              {details.map((d, idx) => (
                <span
                  key={d.key}
                  className="inline-flex items-center min-w-0 gap-1.5 text-xs text-n-slate-11 max-w-full"
                  title={d.label}
                >
                  {idx > 0 && <span className="w-px h-3 bg-n-slate-6 mr-1 shrink-0" />}
                  <Icon icon={d.icon} className="size-3.5 shrink-0 text-n-slate-10" />
                  <span className="truncate max-w-60">{d.label}</span>
                </span>
              ))}
            </div>
          )}
        </div>
      </div>

      {company.last_activity_at && (
        <span className="shrink-0 text-xs tabular-nums text-n-slate-11">
          {timeAgo(company.last_activity_at)}
        </span>
      )}
    </div>
  );
}
