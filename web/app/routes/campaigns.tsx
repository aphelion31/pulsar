// Live chat campaigns list, create/edit dialog and delete. Mirrors Chatwoot's campaigns live chat page
// (routes/dashboard/campaigns/pages/LiveChatCampaignsPage.vue, components-next/Campaigns/*).
// SMS and WhatsApp tabs are shown but disabled until those channels exist in Pulsar.
import { useEffect, useState, type FormEvent } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import Switch from "~/components/next/Switch";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "./shell";

type Campaign = {
  id: number;
  title: string;
  message: string;
  enabled: boolean;
  inbox: { id: number; name: string; channel_type: string } | null;
  sender: { id: number; name: string } | null;
  trigger_rules: { url?: string; time_on_page?: number };
  trigger_only_during_business_hours: boolean;
  campaign_status: string;
  campaign_type: string;
};
type Inbox = { id: number; name: string; channel_type: string };
type Member = { id: number; name: string };
type Form = { title: string; message: string; inboxId: string; senderId: string; enabled: boolean; businessHours: boolean; url: string; timeOnPage: string };

const WEBSITE = "Channel::WebWidget";
const empty: Form = { title: "", message: "", inboxId: "", senderId: "0", enabled: true, businessHours: false, url: "", timeOnPage: "10" };
const input = "rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm";

function validUrl(value: string) {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export default function CampaignsPage() {
  const { me, accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/campaigns`;
  const isAdmin = me.accounts.find(a => a.id === accountId)?.role === "administrator";
  const [items, setItems] = useState<Campaign[]>([]);
  const [inboxes, setInboxes] = useState<Inbox[]>([]);
  const [members, setMembers] = useState<Member[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [mode, setMode] = useState<"create" | "edit" | "delete" | null>(null);
  const [selected, setSelected] = useState<Campaign | null>(null);
  const [form, setForm] = useState<Form>(empty);
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);

  async function load() {
    const [list, inboxData] = await Promise.all([
      api<Campaign[]>(base),
      api<{ payload: Inbox[] }>(`${acct(accountId)}/inboxes`),
    ]);
    setItems(list);
    setInboxes(inboxData.payload.filter(i => i.channel_type === WEBSITE));
  }

  useEffect(() => {
    setLoading(true);
    load().catch(e => setError(e.message)).finally(() => setLoading(false));
  }, [base]);

  useEffect(() => {
    if (!form.inboxId) {
      setMembers([]);
      return;
    }
    let active = true;
    api<{ payload: Member[] }>(`${acct(accountId)}/inbox_members/${form.inboxId}`)
      .then(data => { if (active) setMembers(data.payload); })
      .catch(() => { if (active) setMembers([]); });
    return () => { active = false; };
  }, [form.inboxId, accountId]);

  const errors = {
    title: !form.title.trim() ? t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.TITLE.ERROR") : "",
    message: !form.message.trim() ? t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.MESSAGE.ERROR") : "",
    inbox: !form.inboxId ? t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.INBOX.ERROR") : "",
    url: !validUrl(form.url) ? t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.END_POINT.ERROR") : "",
    timeOnPage: !form.timeOnPage || Number(form.timeOnPage) < 0 ? t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.TIME_ON_PAGE.ERROR") : "",
  };
  const valid = !Object.values(errors).some(Boolean);

  function open(campaign?: Campaign) {
    setError(""); setTouched(false); setSelected(campaign ?? null);
    setForm(campaign ? {
      title: campaign.title,
      message: campaign.message,
      inboxId: String(campaign.inbox?.id ?? ""),
      senderId: String(campaign.sender?.id ?? 0),
      enabled: campaign.enabled,
      businessHours: campaign.trigger_only_during_business_hours,
      url: campaign.trigger_rules?.url ?? "",
      timeOnPage: String(campaign.trigger_rules?.time_on_page ?? 10),
    } : empty);
    setMode(campaign ? "edit" : "create");
  }

  async function save(e: FormEvent) {
    e.preventDefault();
    setTouched(true);
    if (!valid) return;
    setBusy(true); setError("");
    const json = {
      title: form.title,
      message: form.message,
      inbox_id: Number(form.inboxId),
      sender_id: Number(form.senderId) || null,
      enabled: form.enabled,
      trigger_only_during_business_hours: form.businessHours,
      trigger_rules: { url: form.url.trim(), time_on_page: Number(form.timeOnPage) },
      ...(mode === "create" ? { campaign_type: "ongoing" } : {}),
    };
    try {
      await api(mode === "create" ? base : `${base}/${selected!.id}`, { method: mode === "create" ? "POST" : "PATCH", json });
      await load();
      setMode(null);
      setNotice(t(mode === "create" ? "CAMPAIGN.LIVE_CHAT.CREATE.FORM.API.SUCCESS_MESSAGE" : "CAMPAIGN.LIVE_CHAT.EDIT.FORM.API.SUCCESS_MESSAGE"));
    } catch (err) {
      setError(err instanceof Error ? err.message : t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.API.ERROR_MESSAGE"));
    } finally {
      setBusy(false);
    }
  }

  async function toggle(campaign: Campaign) {
    setError(""); setNotice("");
    try {
      const updated = await api<Campaign>(`${base}/${campaign.id}`, { method: "PATCH", json: { enabled: !campaign.enabled } });
      setItems(list => list.map(c => (c.id === campaign.id ? updated : c)));
    } catch (err) {
      setError(err instanceof Error ? err.message : t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.API.ERROR_MESSAGE"));
    }
  }

  async function remove() {
    if (!selected) return;
    setBusy(true); setError("");
    try {
      await api(`${base}/${selected.id}`, { method: "DELETE" });
      await load();
      setMode(null);
      setNotice(t("CAMPAIGN.CONFIRM_DELETE.API.SUCCESS_MESSAGE"));
    } catch {
      setError(t("CAMPAIGN.CONFIRM_DELETE.API.ERROR_MESSAGE"));
    } finally {
      setBusy(false);
    }
  }

  const fieldMsg = (key: keyof typeof errors) => (touched && errors[key] ? <p role="alert" className="text-sm text-n-ruby-11">{errors[key]}</p> : null);

  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
      <h1 className="text-xl font-medium">{t("CAMPAIGN.LIVE_CHAT.HEADER_TITLE")}</h1>
      {isAdmin && <Button sm label={t("CAMPAIGN.LIVE_CHAT.NEW_CAMPAIGN")} icon="plus" onClick={() => open()} />}
    </header>

    {notice && !mode && <p role="status" className="mb-4 text-sm text-n-teal-11">{notice}</p>}
    {error && !mode && <p role="alert" className="mb-4 text-sm text-n-ruby-11">{error}</p>}

    {loading ? <p className="text-sm text-n-slate-11">{t("CAMPAIGN.LIVE_CHAT.HEADER_TITLE")}</p> : items.length === 0 ? <div className="pt-6 text-center">
      <h2 className="text-base font-medium">{t("CAMPAIGN.LIVE_CHAT.EMPTY_STATE.TITLE")}</h2>
      <p className="mt-2 text-sm text-n-slate-11">{t("CAMPAIGN.LIVE_CHAT.EMPTY_STATE.SUBTITLE")}</p>
    </div> : <ul className="flex flex-col gap-4">
      {items.map(c => <li key={c.id} className="flex flex-col gap-3 rounded-xl border border-n-weak bg-n-background p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <h2 className="truncate font-medium">{c.title}</h2>
            <p className="mt-1 whitespace-pre-wrap break-words text-sm text-n-slate-11">{c.message}</p>
          </div>
          <div className="flex shrink-0 items-center gap-3">
            <span className="text-sm text-n-slate-11">{t(c.enabled ? "CAMPAIGN.LIVE_CHAT.CARD.STATUS.ENABLED" : "CAMPAIGN.LIVE_CHAT.CARD.STATUS.DISABLED")}</span>
            <Switch modelValue={c.enabled} disabled={!isAdmin} aria-label={c.title} onModelValueChange={() => toggle(c)} />
            {isAdmin && <>
              <Button sm slate ghost icon="pencil" aria-label={t("CAMPAIGN.LIVE_CHAT.EDIT.TITLE")} onClick={() => open(c)} />
              <Button sm slate ghost icon="trash-2" aria-label={t("CAMPAIGN.CONFIRM_DELETE.TITLE")} onClick={() => { setError(""); setSelected(c); setMode("delete"); }} />
            </>}
          </div>
        </div>
        <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-n-slate-11">
          <span>{t("CAMPAIGN.LIVE_CHAT.CARD.CAMPAIGN_DETAILS.SENT_BY")} <strong className="font-medium text-n-slate-12">{c.sender?.name || t("CAMPAIGN.LIVE_CHAT.CARD.CAMPAIGN_DETAILS.BOT")}</strong> {t("CAMPAIGN.LIVE_CHAT.CARD.CAMPAIGN_DETAILS.FROM")} <strong className="font-medium text-n-slate-12">{c.inbox?.name}</strong></span>
          <span className="break-all">{t("CAMPAIGN.LIVE_CHAT.CARD.CAMPAIGN_DETAILS.URL")} {c.trigger_rules?.url}</span>
          <span>{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.TIME_ON_PAGE.LABEL")}: {c.trigger_rules?.time_on_page}</span>
        </div>
      </li>)}
    </ul>}

    {mode && mode !== "delete" && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onMouseDown={e => { if (e.target === e.currentTarget && !busy) setMode(null); }}>
      <div role="dialog" aria-modal="true" aria-labelledby="campaign-dialog-title" className="max-h-full w-full max-w-lg overflow-auto rounded-xl bg-n-background p-6 shadow-xl">
        <h2 id="campaign-dialog-title" className="text-lg font-medium">{t(mode === "create" ? "CAMPAIGN.LIVE_CHAT.CREATE.TITLE" : "CAMPAIGN.LIVE_CHAT.EDIT.TITLE")}</h2>
        <form className="mt-4 grid gap-4" onSubmit={save} noValidate>
          <label className="grid gap-1 text-sm">{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.TITLE.LABEL")}
            <input className={input} value={form.title} placeholder={t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.TITLE.PLACEHOLDER")} onChange={e => setForm({ ...form, title: e.target.value })} />
            {fieldMsg("title")}
          </label>
          <label className="grid gap-1 text-sm">{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.MESSAGE.LABEL")}
            <textarea rows={3} className={input} value={form.message} placeholder={t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.MESSAGE.PLACEHOLDER")} onChange={e => setForm({ ...form, message: e.target.value })} />
            {fieldMsg("message")}
          </label>
          <label className="grid gap-1 text-sm">{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.INBOX.LABEL")}
            <select className={input} value={form.inboxId} onChange={e => setForm({ ...form, inboxId: e.target.value, senderId: "0" })}>
              <option value="">{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.INBOX.PLACEHOLDER")}</option>
              {inboxes.map(i => <option key={i.id} value={i.id}>{i.name}</option>)}
            </select>
            {fieldMsg("inbox")}
          </label>
          <label className="grid gap-1 text-sm">{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.SENT_BY.LABEL")}
            <select className={input} value={form.senderId} disabled={!form.inboxId} onChange={e => setForm({ ...form, senderId: e.target.value })}>
              <option value="0">{t("CAMPAIGN.LIVE_CHAT.CARD.CAMPAIGN_DETAILS.BOT")}</option>
              {members.map(m => <option key={m.id} value={m.id}>{m.name}</option>)}
            </select>
          </label>
          <label className="grid gap-1 text-sm">{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.END_POINT.LABEL")}
            <input type="url" className={input} value={form.url} placeholder={t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.END_POINT.PLACEHOLDER")} onChange={e => setForm({ ...form, url: e.target.value })} />
            {fieldMsg("url")}
          </label>
          <label className="grid gap-1 text-sm">{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.TIME_ON_PAGE.LABEL")}
            <input type="number" min={0} className={input} value={form.timeOnPage} placeholder={t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.TIME_ON_PAGE.PLACEHOLDER")} onChange={e => setForm({ ...form, timeOnPage: e.target.value })} />
            {fieldMsg("timeOnPage")}
          </label>
          <fieldset className="grid gap-2 text-sm">
            <legend className="mb-1 font-medium">{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.OTHER_PREFERENCES.TITLE")}</legend>
            <label className="flex items-center gap-2"><input type="checkbox" checked={form.enabled} onChange={e => setForm({ ...form, enabled: e.target.checked })} />{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.OTHER_PREFERENCES.ENABLED")}</label>
            <label className="flex items-center gap-2"><input type="checkbox" checked={form.businessHours} onChange={e => setForm({ ...form, businessHours: e.target.checked })} />{t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.OTHER_PREFERENCES.TRIGGER_ONLY_BUSINESS_HOURS")}</label>
          </fieldset>
          {error && <p role="alert" className="text-sm text-n-ruby-11">{error}</p>}
          <div className="flex justify-end gap-2">
            <Button type="button" slate faded label={t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.BUTTONS.CANCEL")} onClick={() => setMode(null)} />
            <Button type="submit" disabled={busy} label={mode === "create" ? t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.BUTTONS.CREATE") : "Update"} />
          </div>
        </form>
      </div>
    </div>}

    {mode === "delete" && selected && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onMouseDown={e => { if (e.target === e.currentTarget && !busy) setMode(null); }}>
      <div role="dialog" aria-modal="true" aria-labelledby="campaign-delete-title" className="w-full max-w-md rounded-xl bg-n-background p-6 shadow-xl">
        <h2 id="campaign-delete-title" className="text-lg font-medium">{t("CAMPAIGN.CONFIRM_DELETE.TITLE")}</h2>
        <p className="my-4 text-sm text-n-slate-11">{t("CAMPAIGN.CONFIRM_DELETE.DESCRIPTION")}</p>
        {error && <p role="alert" className="mb-4 text-sm text-n-ruby-11">{error}</p>}
        <div className="flex justify-end gap-2">
          <Button slate faded label={t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.BUTTONS.CANCEL")} onClick={() => setMode(null)} />
          <Button ruby disabled={busy} label={t("CAMPAIGN.CONFIRM_DELETE.CONFIRM")} onClick={remove} />
        </div>
      </div>
    </div>}
  </section>;
}
