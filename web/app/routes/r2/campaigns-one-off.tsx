// One-off (SMS) campaigns list, creation modal, and delete. Mirrors Chatwoot's SMSCampaignsPage.vue.
import { useEffect, useState, type FormEvent } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { CampaignsTabs } from "~/components/next/CampaignsTabs";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type OneOff = {
  id: number;
  title: string;
  message: string;
  enabled: boolean;
  inbox: { id: number; name: string } | null;
  campaign_status: string;
  campaign_type: string;
  scheduled_at?: number;
};

type SmsInbox = { id: number; name: string; channel_type: string };
type LabelOpt = { id: number; title: string; color?: string };

export default function OneOffCampaignsPage() {
  const { me, accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/campaigns`;
  const isAdmin = me.accounts.find((a) => a.id === accountId)?.role === "administrator";
  const [items, setItems] = useState<OneOff[]>([]);
  const [smsInboxes, setSmsInboxes] = useState<SmsInbox[]>([]);
  const [labels, setLabels] = useState<LabelOpt[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selected, setSelected] = useState<OneOff | null>(null);
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [message, setMessage] = useState("");
  const [inboxId, setInboxId] = useState("");
  const [scheduledAt, setScheduledAt] = useState("");
  const [audienceIds, setAudienceIds] = useState<number[]>([]);
  const [busy, setBusy] = useState(false);

  async function load() {
    const [all, inboxRes, labelRes] = await Promise.all([
      api<OneOff[]>(base),
      api<{ payload: SmsInbox[] }>(`${acct(accountId)}/inboxes`).catch(() => ({ payload: [] })),
      api<{ payload?: LabelOpt[] } | LabelOpt[]>(`${acct(accountId)}/labels`).catch(() => []),
    ]);
    setItems(all.filter((c) => c.campaign_type === "one_off"));
    const filteredInboxes = (inboxRes.payload ?? []).filter(
      (i) => i.channel_type === "Channel::Sms" || i.channel_type === "Channel::TwilioSms",
    );
    setSmsInboxes(filteredInboxes);
    const labelList = Array.isArray(labelRes) ? labelRes : (labelRes.payload ?? []);
    setLabels(labelList);
  }

  useEffect(() => {
    setLoading(true);
    load().catch((e) => setError(e instanceof Error ? e.message : "Request failed")).finally(() => setLoading(false));
  }, [base]);

  function openCreate() {
    setError("");
    setNotice("");
    setTitle("");
    setMessage("");
    setInboxId(smsInboxes[0] ? String(smsInboxes[0].id) : "");
    setScheduledAt("");
    setAudienceIds([]);
    setCreating(true);
  }

  function toggleAudience(id: number) {
    setAudienceIds((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));
  }

  async function createCampaign(e: FormEvent) {
    e.preventDefault();
    if (!title.trim() || !message.trim() || !inboxId) {
      setError("Title, message, and SMS inbox are required");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const scheduledTs = scheduledAt ? Math.floor(new Date(scheduledAt).getTime() / 1000) : undefined;
      await api(base, {
        method: "POST",
        json: {
          title: title.trim(),
          message: message.trim(),
          inbox_id: Number(inboxId),
          campaign_type: "one_off",
          enabled: true,
          audience: audienceIds.map((id) => ({ id, type: "Label" })),
          ...(scheduledTs && Number.isFinite(scheduledTs) ? { scheduled_at: scheduledTs } : {}),
        },
      });
      setCreating(false);
      setNotice("SMS campaign scheduled.");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create SMS campaign");
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/${selected.id}`, { method: "DELETE" });
      setSelected(null);
      await load();
    } catch {
      setError(t("CAMPAIGN.CONFIRM_DELETE.API.ERROR_MESSAGE"));
    } finally {
      setBusy(false);
    }
  }

  const header = t("CAMPAIGN.SMS.HEADER_TITLE");

  return (
    <section className="flex-1 overflow-auto p-8 text-n-slate-12">
      <CampaignsTabs accountId={accountId} />
      <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <h1 className="text-xl font-medium">{header}</h1>
        {isAdmin && (
          <Button sm icon="plus" label={t("CAMPAIGN.SMS.NEW_CAMPAIGN") || "Create SMS Campaign"} onClick={openCreate} />
        )}
      </header>
      {notice && !creating && <p role="status" className="mb-4 text-sm text-n-teal-11">{notice}</p>}
      {error && !selected && !creating && <p role="alert" className="mb-4 text-sm text-n-ruby-11">{error}</p>}

      {loading ? (
        <p className="text-sm text-n-slate-11">{header}</p>
      ) : items.length === 0 ? (
        <div className="pt-6 text-center">
          <h2 className="text-base font-medium">{t("CAMPAIGN.SMS.EMPTY_STATE.TITLE")}</h2>
          <p className="mt-2 text-sm text-n-slate-11">{t("CAMPAIGN.SMS.EMPTY_STATE.SUBTITLE")}</p>
        </div>
      ) : (
        <ul className="flex flex-col gap-4">
          {items.map((c) => (
            <li key={c.id} className="flex flex-col gap-3 rounded-xl border border-n-weak bg-n-background p-4">
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <h2 className="truncate font-medium">{c.title}</h2>
                  <p className="mt-1 whitespace-pre-wrap break-words text-sm text-n-slate-11">{c.message}</p>
                </div>
                {isAdmin && (
                  <Button sm slate ghost icon="trash-2" aria-label={t("CAMPAIGN.CONFIRM_DELETE.TITLE")} onClick={() => { setError(""); setSelected(c); }} />
                )}
              </div>
              <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-n-slate-11">
                <span>{t(c.enabled ? "CAMPAIGN.LIVE_CHAT.CARD.STATUS.ENABLED" : "CAMPAIGN.LIVE_CHAT.CARD.STATUS.DISABLED")}</span>
                {c.inbox && <span>{c.inbox.name}</span>}
              </div>
            </li>
          ))}
        </ul>
      )}

      {creating && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onMouseDown={(e) => { if (e.target === e.currentTarget && !busy) setCreating(false); }}>
          <div role="dialog" aria-modal="true" aria-labelledby="sms-campaign-dialog-title" className="max-h-full w-full max-w-lg overflow-auto rounded-xl bg-n-background p-6 shadow-xl">
            <h2 id="sms-campaign-dialog-title" className="text-lg font-medium">Create SMS Campaign</h2>
            <form className="mt-4 grid gap-4" onSubmit={createCampaign}>
              <label className="grid gap-1 text-sm">
                Title
                <input
                  required
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  placeholder="Holiday announcement"
                  className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm"
                />
              </label>
              <label className="grid gap-1 text-sm">
                Message
                <textarea
                  required
                  rows={3}
                  value={message}
                  onChange={(e) => setMessage(e.target.value)}
                  placeholder="Write your SMS message..."
                  className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm"
                />
              </label>
              <label className="grid gap-1 text-sm">
                SMS Inbox
                <select
                  required
                  value={inboxId}
                  onChange={(e) => setInboxId(e.target.value)}
                  className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm"
                >
                  <option value="">Select SMS inbox</option>
                  {smsInboxes.map((i) => (
                    <option key={i.id} value={i.id}>
                      {i.name} ({i.channel_type.replace("Channel::", "")})
                    </option>
                  ))}
                </select>
              </label>
              <label className="grid gap-1 text-sm">
                Schedule time (optional)
                <input
                  type="datetime-local"
                  value={scheduledAt}
                  onChange={(e) => setScheduledAt(e.target.value)}
                  className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm"
                />
              </label>
              {labels.length > 0 && (
                <fieldset className="grid gap-2 text-sm">
                  <legend className="font-medium">Audience Labels</legend>
                  <div className="flex flex-wrap gap-2">
                    {labels.map((lbl) => (
                      <label key={lbl.id} className="inline-flex items-center gap-1.5 rounded-lg border border-n-weak px-2.5 py-1 text-xs">
                        <input
                          type="checkbox"
                          checked={audienceIds.includes(lbl.id)}
                          onChange={() => toggleAudience(lbl.id)}
                        />
                        {lbl.title}
                      </label>
                    ))}
                  </div>
                </fieldset>
              )}
              {error && <p role="alert" className="text-sm text-n-ruby-11">{error}</p>}
              <div className="flex justify-end gap-2">
                <Button type="button" slate faded label={t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.BUTTONS.CANCEL")} onClick={() => setCreating(false)} />
                <Button type="submit" disabled={busy} label={busy ? "Creating..." : "Create campaign"} />
              </div>
            </form>
          </div>
        </div>
      )}

      {selected && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onMouseDown={(e) => { if (e.target === e.currentTarget && !busy) setSelected(null); }}>
          <div role="dialog" aria-modal="true" aria-labelledby="campaign-delete-title" className="w-full max-w-md rounded-xl bg-n-background p-6 shadow-xl">
            <h2 id="campaign-delete-title" className="text-lg font-medium">{t("CAMPAIGN.CONFIRM_DELETE.TITLE")}</h2>
            <p className="my-4 text-sm text-n-slate-11">{t("CAMPAIGN.CONFIRM_DELETE.DESCRIPTION")}</p>
            {error && <p role="alert" className="mb-4 text-sm text-n-ruby-11">{error}</p>}
            <div className="flex justify-end gap-2">
              <Button slate faded label={t("CAMPAIGN.LIVE_CHAT.CREATE.FORM.BUTTONS.CANCEL")} onClick={() => setSelected(null)} />
              <Button ruby disabled={busy} label={t("CAMPAIGN.CONFIRM_DELETE.CONFIRM")} onClick={remove} />
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
