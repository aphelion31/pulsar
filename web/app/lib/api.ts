// Pulsar API client. devise_token_auth headers persisted in localStorage.
export type Session = { "access-token": string; client: string; uid: string; expiry?: string };

const KEY = "pulsar.session";

export function getSession(): Session | null {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? "null");
    return s && s["access-token"] ? s : null;
  } catch {
    return null;
  }
}

export function clearSession() {
  localStorage.removeItem(KEY);
}

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

function errorMessage(body: unknown, status: number): string {
  if (body && typeof body === "object") {
    const b = body as { error?: string; message?: string; errors?: string[] };
    if (b.errors?.length) return b.errors.join(", ");
    if (b.error) return b.error;
    if (b.message) return b.message;
  }
  return `Request failed (${status})`;
}

export async function api<T = any>(path: string, init: RequestInit & { json?: unknown } = {}): Promise<T> {
  const headers = new Headers(init.headers);
  const s = getSession();
  if (s) {
    headers.set("access-token", s["access-token"]);
    headers.set("client", s.client);
    headers.set("uid", s.uid);
  }
  let body = init.body;
  if (init.json !== undefined) {
    headers.set("content-type", "application/json");
    body = JSON.stringify(init.json);
  }
  const res = await fetch(path, { ...init, headers, body });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (res.status === 401 && path !== "/auth/sign_in") {
    clearSession();
    if (!location.pathname.endsWith("/login")) location.assign("/app/login");
  }
  if (!res.ok) throw new ApiError(res.status, errorMessage(data, res.status));
  return data as T;
}

export async function signIn(email: string, password: string) {
  const res = await fetch("/auth/sign_in", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ email, password }),
  });
  return authResponse(res);
}

export async function authResponse(res: Response) {
  const data = await res.json().catch(() => null);
  if (!res.ok) throw new ApiError(res.status, errorMessage(data, res.status));
  const h = res.headers;
  const session: Session = {
    "access-token": h.get("access-token") ?? "",
    client: h.get("client") ?? "",
    uid: h.get("uid") ?? "",
    expiry: h.get("expiry") ?? undefined,
  };
  localStorage.setItem(KEY, JSON.stringify(session));
  return data.data as Profile;
}

export async function signOut() {
  await api("/auth/sign_out", { method: "DELETE" }).catch(() => {});
  clearSession();
}

// ---------- types (subset of the Chatwoot payloads Pulsar returns) ----------

export type Account = { id: number; name: string; role: "administrator" | "agent"; availability: string };
export type Profile = {
  id: number;
  name: string;
  available_name: string;
  email: string;
  avatar_url: string;
  pubsub_token: string;
  account_id: number;
  accounts: Account[];
};
export type Contact = {
  id: number;
  name: string | null;
  email: string | null;
  phone_number: string | null;
  identifier?: string | null;
  thumbnail?: string;
  created_at?: number;
  last_activity_at?: number | null;
};
export type Agent = { id: number; name: string; available_name: string; email: string; role: string; availability_status: string };
export type Attachment = { id: number; file_type: string; data_url: string; thumb_url: string; extension: string | null; file_size: number };
export type Message = {
  id: number;
  content: string | null;
  message_type: 0 | 1 | 2 | 3;
  private: boolean;
  created_at: number;
  conversation_id: number;
  sender?: { id: number; name: string; type: "user" | "contact" };
  attachments?: Attachment[];
  echo_id?: string;
  content_attributes?: { deleted?: boolean };
};
export type Conversation = {
  id: number;
  inbox_id: number;
  status: "open" | "resolved" | "pending" | "snoozed";
  priority: string | null;
  labels: string[];
  unread_count: number;
  timestamp: number;
  messages: Message[];
  last_non_activity_message: Message | null;
  meta: { sender: Contact; assignee?: Agent; team?: { id: number; name: string } };
};
export type Inbox = { id: number; name: string; channel_type: string; website_token?: string; web_widget_script?: string };
export type Label = { id: number; title: string; description: string | null; color: string; show_on_sidebar: boolean | null };
export type Team = { id: number; name: string; description: string | null };
export type Canned = { id: number; short_code: string; content: string };

export const acct = (id: string | number) => `/api/v1/accounts/${id}`;
