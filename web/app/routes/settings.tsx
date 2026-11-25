import { useEffect, useState } from "react";
import { NavLink, useOutletContext, useParams } from "react-router";
import { Button, ErrorText, Input, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "./shell";

type Item = { id: number; [key: string]: unknown };
type Field = { key: string; label: string; type?: string; options?: string[]; required?: boolean };
const sections: Record<string, { title: string; singular: string; fields: Field[] }> = {
  inboxes: { title: "Inboxes", singular: "inbox", fields: [{ key: "name", label: "Name", required: true }, { key: "channel_type", label: "Channel", options: ["api", "web_widget"] }, { key: "website_url", label: "Website URL", type: "url" }, { key: "webhook_url", label: "Webhook URL", type: "url" }] },
  agents: { title: "Agents", singular: "agent", fields: [{ key: "name", label: "Name", required: true }, { key: "email", label: "Email", type: "email", required: true }, { key: "role", label: "Role", options: ["agent", "administrator"] }] },
  teams: { title: "Teams", singular: "team", fields: [{ key: "name", label: "Name", required: true }, { key: "description", label: "Description" }] },
  labels: { title: "Labels", singular: "label", fields: [{ key: "title", label: "Title", required: true }, { key: "description", label: "Description" }, { key: "color", label: "Color", type: "color" }] },
  canned_responses: { title: "Canned responses", singular: "canned response", fields: [{ key: "short_code", label: "Short code", required: true }, { key: "content", label: "Content", required: true }] },
};
const text = (e: unknown) => e instanceof Error ? e.message : "Request failed";

export default function Settings() {
  const { accountId, me } = useOutletContext<ShellCtx>();
  const { section } = useParams();
  const active = section && sections[section] ? section : "inboxes";
  const admin = me.accounts.find((a) => a.id === accountId)?.role === "administrator";
  return <div className="flex min-h-0 flex-1 flex-col"><PageHeader title="Settings" /><div className="flex min-h-0 flex-1"><nav aria-label="Settings sections" className="w-48 shrink-0 border-r border-line p-4">{Object.entries(sections).map(([key, value]) => <NavLink key={key} to={`/accounts/${accountId}/settings/${key}`} className={`block rounded-lg px-3 py-2 text-[13px] transition-colors ${active === key ? "bg-accent-soft text-ink" : "text-ink-2 hover:bg-bg"}`}>{value.title}</NavLink>)}</nav><Section key={`${accountId}-${active}`} section={active} accountId={accountId} admin={admin} /></div></div>;
}

function Section({ section, accountId, admin }: { section: string; accountId: number; admin: boolean }) {
  const config = sections[section]; const base = `${acct(accountId)}/${section}`;
  const defaults = () => Object.fromEntries(config.fields.map((f) => [f.key, f.type === "color" ? "#64748b" : f.options?.[0] ?? ""]));
  const [form, setForm] = useState<Record<string, string>>(defaults); const [items, setItems] = useState<Item[]>([]); const [editing, setEditing] = useState<number | null>(null); const [open, setOpen] = useState(false); const [error, setError] = useState(""); const [busy, setBusy] = useState(false);
  async function load() { const r = await api<Item[] | { payload: Item[] }>(base); setItems(Array.isArray(r) ? r : r.payload); }
  useEffect(() => { load().catch((e) => setError(text(e))); }, [base]);
  function edit(item: Item) { setEditing(item.id); setForm(Object.fromEntries(config.fields.map((f) => [f.key, String(item[f.key] ?? (f.type === "color" ? "#64748b" : ""))]))); setOpen(true); }
  async function save(e: React.FormEvent) {
    e.preventDefault(); setBusy(true); setError("");
    try {
      let data: Record<string, unknown> = { ...form };
      if (section === "inboxes") data = { name: form.name, channel: { type: form.channel_type, ...(form.website_url ? { website_url: form.website_url } : {}), ...(form.webhook_url ? { webhook_url: form.webhook_url } : {}) } };
      if (section === "agents" && editing !== null) data = { role: form.role };
      await api(editing === null ? base : `${base}/${editing}`, { method: editing === null ? "POST" : "PATCH", json: data });
      setOpen(false); setEditing(null); setForm(defaults()); await load();
    } catch (e) { setError(text(e)); } finally { setBusy(false); }
  }
  async function remove(item: Item) { if (!window.confirm(`Delete this ${config.singular}?`)) return; setError(""); try { await api(`${base}/${item.id}`, { method: "DELETE" }); await load(); } catch (e) { setError(text(e)); } }
  const columns = section === "inboxes" ? ["name", "channel_type"] : section === "agents" ? ["name", "email", "role", "availability_status"] : config.fields.map((f) => f.key);
  const label = (key: string) => config.fields.find((f) => f.key === key)?.label ?? "Availability";
  return <section className="min-w-0 flex-1 overflow-auto p-6"><div className="mb-4 flex items-center justify-between"><h2 className="text-[15px] font-medium">{config.title}</h2>{admin && <Button onClick={() => { setEditing(null); setForm(defaults()); setOpen(!open); }}>New {config.singular}</Button>}</div><ErrorText>{error}</ErrorText>{admin && open && <form onSubmit={save} className="mb-4 grid gap-3 rounded-lg border border-line bg-bg p-4"><h3 className="text-[13px] font-medium">{editing === null ? `New ${config.singular}` : `Edit ${config.singular}`}</h3><div className="grid gap-3 sm:grid-cols-2">{config.fields.filter((f) => !(editing !== null && section === "agents" && f.key !== "role")).map((f) => f.options ? <label key={f.key} className="grid gap-1.5 text-[13px] text-ink-2">{f.label}<select value={form[f.key]} onChange={(e) => setForm({ ...form, [f.key]: e.target.value })} className="h-9 rounded-lg border border-line-strong bg-surface px-3 text-[14px] text-ink"><option value={f.options[0]}>{f.options[0] === "api" ? "API" : f.options[0]}</option>{f.options.slice(1).map((v) => <option key={v} value={v}>{v === "web_widget" ? "Website" : v}</option>)}</select></label> : <Input key={f.key} label={f.label} type={f.type} required={f.required} value={form[f.key]} onChange={(e) => setForm({ ...form, [f.key]: e.target.value })} />)}</div><div className="flex justify-end gap-2"><Button type="button" variant="quiet" onClick={() => setOpen(false)}>Cancel</Button><Button disabled={busy}>{editing === null ? "Create" : "Save changes"}</Button></div></form>}
  <div className="overflow-x-auto rounded-lg border border-line"><table className="w-full text-left text-[13px]"><thead className="bg-bg text-ink-3"><tr>{columns.map((c) => <th key={c} className="px-4 py-3 font-medium">{label(c)}</th>)}{admin && <th className="px-4 py-3 font-medium">Actions</th>}</tr></thead><tbody>{items.map((item) => <tr key={item.id} className="border-t border-line">{columns.map((c) => <td key={c} className="px-4 py-3 text-ink-2">{c === "color" ? <span className="inline-block h-3 w-3 rounded-sm border border-line" style={{ backgroundColor: String(item[c] ?? "transparent") }} aria-label={`Color ${item[c]}`} /> : c === "channel_type" ? String(item[c]).toLowerCase().includes("api") ? "API" : "Website" : String(item[c] ?? "—")}</td>)}{admin && <td className="whitespace-nowrap px-4 py-2">{section !== "inboxes" && <Button variant="quiet" onClick={() => edit(item)}>Edit</Button>}<Button variant="danger" onClick={() => remove(item)}>Delete</Button></td>}</tr>)}</tbody></table>{items.length === 0 && <p className="p-4 text-[13px] text-ink-3">No {config.title.toLowerCase()}</p>}</div></section>;
}
