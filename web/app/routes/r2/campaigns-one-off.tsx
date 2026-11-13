// One-off (SMS) campaigns list. mirrors chatwoot/app/javascript/dashboard/routes/dashboard/campaigns/pages/SMSCampaignsPage.vue.
// Backend: GET /api/v1/accounts/:id/campaigns returns all types; filter client side by campaign_type "one_off".
// Pulsar has no SMS channel, so this is a read-only list with delete (admin) like Chatwoot's ConfirmDeleteCampaignDialog.
import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
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

export default function OneOffCampaignsPage() {
  const { me, accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/campaigns`;
  const isAdmin = me.accounts.find((a) => a.id === accountId)?.role === "administrator";
  const [items, setItems] = useState<OneOff[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<OneOff | null>(null);
  const [busy, setBusy] = useState(false);

  async function load() {
    const all = await api<OneOff[]>(base);
    setItems(all.filter((c) => c.campaign_type === "one_off"));
  }

  useEffect(() => {
    setLoading(true);
    load().catch((e) => setError(e instanceof Error ? e.message : "Request failed")).finally(() => setLoading(false));
  }, [base]);

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
      <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <h1 className="text-xl font-medium">{header}</h1>
      </header>
      {error && !selected && <p role="alert" className="mb-4 text-sm text-n-ruby-11">{error}</p>}

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
