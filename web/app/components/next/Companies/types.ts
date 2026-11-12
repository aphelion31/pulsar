export type CompanyItem = {
  id: number;
  name: string;
  domain: string | null;
  description: string | null;
  contacts_count: number;
  avatar_url?: string;
  created_at?: number;
  updated_at?: number;
  last_activity_at?: number | null;
  additional_attributes?: Record<string, any>;
  custom_attributes?: Record<string, any>;
};

export type CompanyContactItem = {
  id: number;
  name: string;
  email: string | null;
  phone_number: string | null;
  thumbnail?: string;
  company_id?: number | null;
  linked_to_current_company?: boolean;
  company?: CompanyItem | null;
  additional_attributes?: Record<string, any>;
  last_activity_at?: number | null;
};
