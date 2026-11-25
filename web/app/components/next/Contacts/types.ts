import type { Contact } from "~/lib/api";

export type ContactItem = Contact & {
  additional_attributes?: Record<string, any>;
  custom_attributes?: Record<string, any>;
  company_id?: number | null;
  blocked?: boolean;
};
