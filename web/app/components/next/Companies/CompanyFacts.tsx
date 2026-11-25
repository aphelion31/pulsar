import Icon from "~/components/next/Icon";
import { timeAgo } from "~/lib/format";
import { t } from "~/lib/i18n";
import type { CompanyItem } from "./types";

type Props = {
  company: CompanyItem;
};

export default function CompanyFacts({ company }: Props) {
  const attrs = (company.additional_attributes ?? {}) as Record<string, any>;
  const customAttrs = (company.custom_attributes ?? {}) as Record<string, any>;

  const industry = attrs.sub_industry || attrs.industry;
  const employees = attrs.employee_count
    ? Number(attrs.employee_count).toLocaleString()
    : attrs.employee_count_range;
  const location = [attrs.city, attrs.country || attrs.country_code]
    .filter(Boolean)
    .join(", ");

  const added = company.created_at
    ? new Date(company.created_at * 1000).toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
        year: "numeric",
      })
    : "";

  const facts: Array<{ key: string; icon: string; label: string; value?: string }> = [
    {
      key: "industry",
      icon: "i-lucide-building-2",
      label: t("COMPANIES.DETAIL.FACTS.INDUSTRY"),
      value: industry,
    },
    {
      key: "size",
      icon: "i-lucide-users",
      label: t("COMPANIES.DETAIL.FACTS.SIZE"),
      value: employees,
    },
    {
      key: "location",
      icon: "i-lucide-map-pin",
      label: t("COMPANIES.DETAIL.FACTS.LOCATION"),
      value: location,
    },
    {
      key: "added",
      icon: "i-lucide-calendar",
      label: t("COMPANIES.DETAIL.FACTS.ADDED"),
      value: added || (company.created_at ? timeAgo(company.created_at) : undefined),
    },
    ...Object.entries(customAttrs)
      .filter(([, val]) => val !== null && val !== undefined && val !== "")
      .map(([key, val]) => ({
        key: `custom-${key}`,
        icon: "i-lucide-tag",
        label: key,
        value: typeof val === "boolean" ? (val ? "Yes" : "No") : String(val),
      })),
  ];

  const columns = 4;
  const emptyCells = (columns - (facts.length % columns)) % columns;

  return (
    <div className="overflow-hidden border rounded-xl border-n-weak bg-n-weak">
      <dl className="grid grid-cols-2 gap-px m-0 md:grid-cols-4">
        {facts.map((fact) => (
          <div
            key={fact.key}
            className="flex flex-col min-w-0 gap-1 px-4 py-3 bg-n-solid-2"
          >
            <dt className="flex items-center gap-1.5 text-xs text-n-slate-10">
              <Icon icon={fact.icon} className="size-3.5 shrink-0" />
              <span className="truncate">{fact.label}</span>
            </dt>
            <dd
              className={`text-sm font-medium truncate ${
                fact.value ? "text-n-slate-12" : "text-n-slate-9"
              }`}
              title={fact.value}
            >
              {fact.value || t("COMPANIES.DETAIL.FACTS.UNKNOWN")}
            </dd>
          </div>
        ))}
        {Array.from({ length: emptyCells }).map((_, idx) => (
          <div key={`empty-${idx}`} className="bg-n-solid-2 hidden md:block" />
        ))}
      </dl>
    </div>
  );
}
