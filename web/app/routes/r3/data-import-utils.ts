// Shared helpers for the data import pages. Mirrors chatwoot/app/javascript/dashboard/routes/dashboard/settings/data/importStatus.js.
import { t } from "~/lib/i18n";

export const POLL_MS = 5000;
export const ACTIVE = ["pending", "processing"];
export const isActive = (status: string) => ACTIVE.includes(status);

export const formatStatus = (value?: string | null) => value?.replaceAll("_", " ") || "-";

export const formatDate = (value?: string | null) => {
  if (!value) return "-";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
};

export const statusDot = (status: string) =>
  (
    {
      pending: "bg-amber-500",
      processing: "bg-blue-500",
      completed: "bg-teal-500",
      completed_with_errors: "bg-amber-500",
      failed: "bg-red-500",
      abandoned: "bg-slate-500",
    } as Record<string, string>
  )[status] ?? "bg-slate-500";

export const importedCount = (item: { processed_records?: number | null }) => Number(item.processed_records || 0);

export const importTypeLabel = (item: { data_type: string; import_types?: string[] }) => {
  const types = item.import_types?.length ? item.import_types : [item.data_type];
  return types.map((type) => (type === "contacts" ? t("DATA_IMPORTS.TYPES.CONTACTS") : type)).join(", ");
};
