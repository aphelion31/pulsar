import { useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import { Plus, X } from "lucide-react";
import { Avatar, Button, Empty, ErrorText, Input, PageHeader } from "~/components/ui";
import { acct, api, type Contact } from "~/lib/api";
import { timeAgo } from "~/lib/format";
import type { ShellCtx } from "./shell";

type Listing = { meta: { count: number; current_page: number }; payload: Contact[] };
type Note = { id: number; content: string; created_at: number; user?: { name: string } };
const message = (e: unknown) => e instanceof Error ? e.message : "Request failed";

export default function Contacts() {
  const { accountId, me } = useOutletContext<ShellCtx>();
  const admin = me.accounts.find((a) => a.id === accountId)?.role === "administrator";
  const base = `${acct(accountId)}/contacts`;
  const [list, setList] = useState<Listing | null>(null);
  const [page, setPage] = useState(1);
  const [sort, setSort] = useState("name");
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Contact | null>(null);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  useEffect(() => { const timer = setTimeout(() => { setSearch(query); setPage(1); }, 250); return () => clearTimeout(timer); }, [query]);
  useEffect(() => {
    let live = true;
    const url = search ? `${base}/search?q=${encodeURIComponent(search)}&page=${page}` : `${base}?page=${page}&sort=${sort}`;
    api<Listing>(url).then((r) => { if (live) { setList(r); setError(""); } }).catch((e) => { if (live) setError(message(e)); });
    return () => { live = false; };
  }, [base, page, sort, search, revision]);
  const refresh = () => setRevision((n) => n + 1);
  const pages = Math.max(1, Math.ceil((list?.meta.count ?? 0) / 15));
  return <div className="flex min-h-0 min-w-0 flex-1">
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title="Contacts"><Input aria-label="Search contacts" placeholder="Search contacts" value={query} onChange={(e) => setQuery(e.target.value)} /><Button className="shrink-0 whitespace-nowrap" onClick={() => { setCreating(true); setSelected(null); }}><Plus size={16} strokeWidth={1.5} />New contact</Button></PageHeader>
      <div className="min-h-0 flex-1 overflow-auto p-6"><ErrorText>{error}</ErrorText>
        {list && list.payload.length === 0 ? <Empty title="No contacts">Contacts will appear here</Empty> : <div className="overflow-x-auto rounded-lg border border-line"><table className="w-full text-left text-[13px]"><thead className="bg-bg text-ink-3"><tr>{[["Name", "name"], ["Email", ""], ["Phone", ""], ["Last activity", "-last_activity_at"]].map(([label, value]) => <th key={label} className="px-4 py-3 font-medium">{value ? <button className="transition-colors hover:text-ink" onClick={() => { setSort(value === "name" && sort === "name" ? "-name" : value); setPage(1); }}>{label}</button> : label}</th>)}</tr></thead><tbody>{list?.payload.map((c) => <tr key={c.id} className="border-t border-line transition-colors hover:bg-bg"><td className="px-4 py-3"><button className="flex items-center gap-2 text-left hover:underline" onClick={() => { setSelected(c); setCreating(false); }}><Avatar name={c.name} size={32} />{c.name || "Unknown contact"}</button></td><td className="px-4 py-3 text-ink-2">{c.email || "—"}</td><td className="px-4 py-3 text-ink-2">{c.phone_number || "—"}</td><td className="num px-4 py-3 text-ink-3">{timeAgo(c.last_activity_at) || "—"}</td></tr>)}</tbody></table></div>}
      </div><div className="flex items-center justify-end gap-2 border-t border-line px-6 py-3"><Button variant="quiet" disabled={page <= 1} onClick={() => setPage(page - 1)}>Prev</Button><span className="num text-[13px] text-ink-3">Page {page} of {pages}</span><Button variant="quiet" disabled={page >= pages} onClick={() => setPage(page + 1)}>Next</Button></div>
    </section>
    {(selected || creating) && <Detail key={selected?.id ?? "new"} base={base} contact={selected} admin={admin} accountId={accountId} onClose={() => { setSelected(null); setCreating(false); }} onSaved={(c) => { setSelected(c); setCreating(false); refresh(); }} onDeleted={() => { setSelected(null); refresh(); }} />}
  </div>;
}

function Detail({ base, contact, admin, accountId, onClose, onSaved, onDeleted }: { base: string; contact: Contact | null; admin: boolean; accountId: number; onClose: () => void; onSaved: (c: Contact) => void; onDeleted: () => void }) {
  const [name, setName] = useState(contact?.name ?? ""); const [email, setEmail] = useState(contact?.email ?? ""); const [phone, setPhone] = useState(contact?.phone_number ?? "");
  const [notes, setNotes] = useState<Note[]>([]); const [conversations, setConversations] = useState<{ id: number; status: string }[]>([]); const [text, setText] = useState(""); const [error, setError] = useState(""); const [busy, setBusy] = useState(false);
  const path = contact ? `${base}/${contact.id}` : base;
  const loadNotes = () => api<Note[] | { payload: Note[] }>(`${path}/notes`).then((r) => setNotes(Array.isArray(r) ? r : r.payload));
  useEffect(() => { if (!contact) return; Promise.all([loadNotes(), api<{ payload: { id: number; status: string }[] }>(`${path}/conversations`).then((r) => setConversations(r.payload))]).catch((e) => setError(message(e))); }, [path]);
  async function save(e: React.FormEvent) { e.preventDefault(); setBusy(true); setError(""); try { const r = await api<{ payload: { contact: Contact } | Contact }>(path, { method: contact ? "PATCH" : "POST", json: { name, email, phone_number: phone } }); const payload = r.payload; onSaved("contact" in payload ? payload.contact : payload); } catch (e) { setError(message(e)); } finally { setBusy(false); } }
  async function addNote(e: React.FormEvent) { e.preventDefault(); setBusy(true); setError(""); try { await api(`${path}/notes`, { method: "POST", json: { content: text } }); setText(""); await loadNotes(); } catch (e) { setError(message(e)); } finally { setBusy(false); } }
  async function remove() { if (!window.confirm("Delete this contact?")) return; try { await api(path, { method: "DELETE" }); onDeleted(); } catch (e) { setError(message(e)); } }
  return <aside className="w-96 max-w-full shrink-0 overflow-y-auto border-l border-line bg-surface"><header className="flex h-14 items-center justify-between border-b border-line px-4"><h2 className="font-medium">{contact ? "Contact details" : "New contact"}</h2><Button variant="quiet" aria-label="Close contact details" onClick={onClose}><X size={16} strokeWidth={1.5} /></Button></header><div className="grid gap-6 p-4"><form onSubmit={save} className="grid gap-3"><Input label="Name" value={name} onChange={(e) => setName(e.target.value)} required disabled={!!contact && !admin} /><Input label="Email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} disabled={!!contact && !admin} /><Input label="Phone" value={phone} onChange={(e) => setPhone(e.target.value)} disabled={!!contact && !admin} /><ErrorText>{error}</ErrorText><div className="flex justify-between">{contact && admin && <Button type="button" variant="danger" onClick={remove}>Delete contact</Button>}{(!contact || admin) && <Button disabled={busy} className="ml-auto">{contact ? "Save changes" : "Create contact"}</Button>}</div></form>{contact && <><section><h3 className="mb-2 text-[13px] font-medium">Conversations</h3>{conversations.length ? <ul className="grid gap-2">{conversations.map((c) => <li key={c.id}><Link className="text-[13px] text-ink-2 hover:underline" to={`/accounts/${accountId}/conversations/${c.id}`}><span className="num">#{c.id}</span> {c.status}</Link></li>)}</ul> : <p className="text-[13px] text-ink-3">No conversations</p>}</section><section><h3 className="mb-2 text-[13px] font-medium">Notes</h3><ul className="grid gap-3">{notes.map((n) => <li key={n.id} className="rounded-lg bg-bg p-3 text-[13px]"><p className="whitespace-pre-wrap">{n.content}</p><p className="num mt-1 text-[11px] text-ink-3">{n.user?.name} {timeAgo(n.created_at)}</p></li>)}</ul><form onSubmit={addNote} className="mt-3 grid gap-2"><textarea aria-label="Note" required value={text} onChange={(e) => setText(e.target.value)} rows={3} className="w-full rounded-lg border border-line-strong bg-surface px-3 py-2 text-[13px] outline-none focus:border-accent" /><Button disabled={busy || !text.trim()} className="justify-self-end">Add note</Button></form></section></>}</div></aside>;
}
