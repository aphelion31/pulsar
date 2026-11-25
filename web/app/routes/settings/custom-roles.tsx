import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

// mirrors chatwoot/app/javascript/dashboard/routes/dashboard/settings/customRoles/
type CustomRole = { id: number; name: string; description: string | null; permissions: string[] };
type Form = { name: string; description: string; permissions: string[] };

const PERMISSIONS = [
  "conversation_manage",
  "conversation_unassigned_manage",
  "conversation_participating_manage",
  "contact_manage",
  "report_manage",
  "knowledge_base_manage",
];
const MANAGE_ALL = "conversation_manage";
const UNASSIGNED = "conversation_unassigned_manage";
const PARTICIPATING = "conversation_participating_manage";
const empty: Form = { name: "", description: "", permissions: [] };

// Picks the singular or plural half of a "{n} x | {n} xs" string.
const plural = (key: string, n: number) => t(key, { n }).split(" | ")[n === 1 ? 0 : 1].trim();

export default function CustomRolesSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/custom_roles`;
  const [items, setItems] = useState<CustomRole[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<CustomRole | null>(null);
  const [mode, setMode] = useState<"add" | "edit" | "delete" | null>(null);
  const [form, setForm] = useState<Form>(empty);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function load() {
    const data = await api<CustomRole[] | { payload: CustomRole[] }>(base);
    setItems(Array.isArray(data) ? data : data.payload);
  }
  useEffect(() => { setLoading(true); load().catch(e => setError(e.message)).finally(() => setLoading(false)); }, [base]);

  function open(item?: CustomRole) {
    setError(""); setSelected(item ?? null);
    setForm(item ? { name: item.name, description: item.description ?? "", permissions: item.permissions ?? [] } : empty);
    setMode(item ? "edit" : "add");
  }

  // Same rule as Chatwoot: "manage all" implies unassigned + participating; removing it only removes itself.
  function toggle(permission: string, checked: boolean) {
    setForm(f => {
      let next = checked ? [...f.permissions, permission] : f.permissions.filter(p => p !== permission);
      if (permission === MANAGE_ALL && checked) next = [...new Set([...next, UNASSIGNED, PARTICIPATING])];
      return { ...f, permissions: next };
    });
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault(); setBusy(true); setError("");
    try {
      const json = { name: form.name.trim(), description: form.description.trim(), permissions: form.permissions };
      await api(mode === "add" ? base : `${base}/${selected!.id}`, { method: mode === "add" ? "POST" : "PATCH", json });
      await load(); setMode(null);
    } catch (e) { setError(e instanceof Error ? e.message : t("CUSTOM_ROLE.FORM.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }

  async function remove() {
    if (!selected) return;
    setBusy(true); setError("");
    try { await api(`${base}/${selected.id}`, { method: "DELETE" }); await load(); setMode(null); }
    catch (e) { setError(e instanceof Error ? e.message : t("CUSTOM_ROLE.DELETE.API.ERROR_MESSAGE")); }
    finally { setBusy(false); }
  }

  const filtered = items.filter(item => `${item.name} ${item.description ?? ""}`.toLowerCase().includes(search.trim().toLowerCase()));
  const valid = form.name.trim().length >= 2 && form.description.trim().length > 0 && form.permissions.length > 0;
  const permLabel = (p: string) => t(`CUSTOM_ROLE.PERMISSIONS.${p.toUpperCase()}`);

  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
      <div><h1 className="text-xl font-medium">{t("CUSTOM_ROLE.HEADER")}</h1><p className="mt-2 text-sm text-n-slate-11">{t("CUSTOM_ROLE.ADD.DESC")}</p></div>
      <Button sm label={t("CUSTOM_ROLE.HEADER_BTN_TXT")} onClick={() => open()} />
    </header>
    <div className="mb-4 flex items-center justify-between gap-3"><span className="text-sm text-n-slate-11">{items.length > 0 && plural("CUSTOM_ROLE.COUNT", items.length)}</span><input aria-label={t("CUSTOM_ROLE.SEARCH_PLACEHOLDER")} placeholder={t("CUSTOM_ROLE.SEARCH_PLACEHOLDER")} value={search} onChange={e => setSearch(e.target.value)} className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm" /></div>
    {error && !mode && <p role="alert" className="mb-4 text-n-ruby-11">{error}</p>}
    {loading ? <p>{t("CUSTOM_ROLE.LOADING")}</p> : items.length === 0 ? <p className="text-sm text-n-slate-11">{t("CUSTOM_ROLE.LIST.404")}</p> : <div className="overflow-x-auto"><table className="w-full text-left text-sm"><thead><tr className="border-b border-n-weak text-n-slate-11">{["NAME", "DESCRIPTION", "PERMISSIONS", "ACTIONS"].map(k => <th className="p-3 font-medium" key={k}>{t(`CUSTOM_ROLE.LIST.TABLE_HEADER.${k}`)}</th>)}</tr></thead><tbody>{filtered.map(item => <tr key={item.id} className="border-b border-n-weak"><td className="p-3">{item.name}</td><td className="w-full p-3 text-n-slate-11">{item.description}</td><td className="p-3 text-n-slate-11">{(item.permissions ?? []).map(permLabel).join(", ")}</td><td className="p-3"><div className="flex justify-end gap-3"><Button sm slate ghost aria-label={t("CUSTOM_ROLE.EDIT.BUTTON_TEXT")} icon="pencil" onClick={() => open(item)} /><Button sm slate ghost aria-label={t("CUSTOM_ROLE.DELETE.BUTTON_TEXT")} icon="trash-2" onClick={() => { setError(""); setSelected(item); setMode("delete"); }} /></div></td></tr>)}</tbody></table>{filtered.length === 0 && <p className="p-4">{t("CUSTOM_ROLE.NO_RESULTS")}</p>}</div>}
    {mode && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onMouseDown={e => { if (e.target === e.currentTarget && !busy) setMode(null); }}><div role="dialog" aria-modal="true" aria-labelledby="custom-roles-dialog-title" className="max-h-[90vh] w-full max-w-lg overflow-auto rounded-xl bg-n-background p-6 shadow-xl"><h2 id="custom-roles-dialog-title" className="text-lg font-medium">{mode === "add" ? t("CUSTOM_ROLE.ADD.TITLE") : mode === "edit" ? t("CUSTOM_ROLE.EDIT.TITLE") : t("CUSTOM_ROLE.DELETE.CONFIRM.TITLE")}</h2>
      {mode === "delete" ? <><p className="my-4">{t("CUSTOM_ROLE.DELETE.CONFIRM.MESSAGE")} {selected?.name}?</p>{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="mt-6 flex justify-end gap-2"><Button slate faded label={`${t("CUSTOM_ROLE.DELETE.CONFIRM.NO")}${selected?.name ?? ""}`} onClick={() => setMode(null)} /><Button ruby label={`${t("CUSTOM_ROLE.DELETE.CONFIRM.YES")}${selected?.name ?? ""}`} disabled={busy} onClick={remove} /></div></> : <form onSubmit={submit} className="mt-4 grid gap-4"><p className="text-sm text-n-slate-11">{mode === "add" ? t("CUSTOM_ROLE.ADD.DESC") : t("CUSTOM_ROLE.EDIT.DESC")}</p><label className="grid gap-1 text-sm">{t("CUSTOM_ROLE.FORM.NAME.LABEL")}<input autoFocus required minLength={2} value={form.name} onChange={e => setForm({ ...form, name: e.target.value })} placeholder={t("CUSTOM_ROLE.FORM.NAME.PLACEHOLDER")} className="rounded-lg border border-n-weak bg-n-background px-3 py-2" /></label><label className="grid gap-1 text-sm">{t("CUSTOM_ROLE.FORM.DESCRIPTION.LABEL")}<textarea required rows={3} value={form.description} onChange={e => setForm({ ...form, description: e.target.value })} placeholder={t("CUSTOM_ROLE.FORM.DESCRIPTION.PLACEHOLDER")} className="rounded-lg border border-n-weak bg-n-background px-3 py-2" /></label><fieldset className="grid gap-2 text-sm"><legend className="mb-1">{t("CUSTOM_ROLE.FORM.PERMISSIONS.LABEL")}</legend>{PERMISSIONS.map(p => <label key={p} className="flex items-center gap-2"><input type="checkbox" name="permissions" value={p} checked={form.permissions.includes(p)} onChange={e => toggle(p, e.target.checked)} />{permLabel(p)}</label>)}</fieldset>{error && <p role="alert" className="text-n-ruby-11">{error}</p>}<div className="flex justify-end gap-2"><Button type="button" slate faded label={t("CUSTOM_ROLE.FORM.CANCEL_BUTTON_TEXT")} onClick={() => setMode(null)} /><Button type="submit" disabled={!valid || busy} label={t(mode === "add" ? "CUSTOM_ROLE.ADD.SUBMIT" : "CUSTOM_ROLE.EDIT.SUBMIT")} /></div></form>}
    </div></div>}
  </section>;
}
