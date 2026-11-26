import { ArrowLeft, BarChart3, Megaphone, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useOutletContext, useParams } from "react-router";
import Button from "~/components/next/Button";
import { CampaignsTabs } from "~/components/next/CampaignsTabs";
import { acct, api, type Inbox, type Label } from "~/lib/api";
import type { ShellCtx } from "./shell";

type Campaign = {
  id: number;
  title: string;
  description?: string | null;
  message: string;
  enabled: boolean;
  campaign_type: string;
  campaign_status: string;
  scheduled_at?: number;
  audience?: Array<{ type: string; id: number }>;
  template_params?: Record<string, unknown> | null;
  inbox?: { id: number; name: string; channel_type?: string } | null;
};

type WaInbox = Inbox & {
  message_templates?: Array<{ name: string; language: string; status?: string; components?: unknown[] }>;
};

type Metrics = {
  audience: number;
  sent: number;
  delivered: number;
  read: number;
  failed: number;
  skipped: number;
  status_counts?: Record<string, number>;
};

type RecipientRow = {
  contact: { id: number; name?: string | null; phone_number?: string | null; email?: string | null };
  status: string;
  message_content?: string | null;
  error_code?: string | null;
  error_title?: string | null;
  error_message?: string | null;
};

export default function WhatsAppCampaignsPage() {
  const { me, accountId } = useOutletContext<ShellCtx>();
  const { campaignId } = useParams();
  const location = useLocation();
  const navigate = useNavigate();
  const base = acct(accountId);
  const isAdmin = me.accounts.find((a) => a.id === accountId)?.role === "administrator";

  const isAnalytics = Boolean(campaignId) && location.pathname.endsWith("/analytics");
  const isNew = location.pathname.endsWith("/campaigns/whatsapp/new");
  const isEdit = Boolean(campaignId) && !isAnalytics;

  const [campaigns, setCampaigns] = useState<Campaign[]>([]);
  const [waInboxes, setWaInboxes] = useState<WaInbox[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [metrics, setMetrics] = useState<Metrics | null>(null);
  const [recipients, setRecipients] = useState<RecipientRow[]>([]);
  const [statusFilter, setStatusFilter] = useState("");
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);

  // Form fields
  const [title, setTitle] = useState("");
  const [inboxId, setInboxId] = useState<string>("");
  const [templateName, setTemplateName] = useState("");
  const [templateLang, setTemplateLang] = useState("en");
  const [message, setMessage] = useState("");
  const [scheduledAt, setScheduledAt] = useState("");
  const [selectedLabelIds, setSelectedLabelIds] = useState<number[]>([]);

  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const loadList = useCallback(async () => {
    const [allC, inboxRes, labelRes] = await Promise.all([
      api<Campaign[]>(`${base}/campaigns`),
      api<{ payload: WaInbox[] }>(`${base}/inboxes`),
      api<{ payload: Label[] } | Label[]>(`${base}/labels`),
    ]);
    const inboxes = (inboxRes.payload || []).filter((i) => i.channel_type === "Channel::Whatsapp");
    setWaInboxes(inboxes);
    const lbls = Array.isArray(labelRes) ? labelRes : labelRes.payload || [];
    setLabels(lbls);
    const waIds = new Set(inboxes.map((i) => i.id));
    setCampaigns(
      allC.filter(
        (c) =>
          c.campaign_type === "one_off" &&
          (c.inbox?.channel_type === "Channel::Whatsapp" || (c.inbox?.id && waIds.has(c.inbox.id)))
      )
    );
    if (!inboxId && inboxes.length > 0 && !isEdit) {
      setInboxId(String(inboxes[0].id));
    }
  }, [base, inboxId, isEdit]);

  const loadAnalytics = useCallback(
    async (id: string, p: number, stFilter: string) => {
      const qs = new URLSearchParams({ page: String(p), per_page: "25" });
      if (stFilter) qs.set("status", stFilter);
      const [c, m, r] = await Promise.all([
        api<Campaign>(`${base}/campaigns/${id}`),
        api<Metrics>(`${base}/campaigns/${id}/analytics/metrics`),
        api<{ payload: RecipientRow[]; meta: { current_page: number; total_pages: number } }>(
          `${base}/campaigns/${id}/analytics/contacts?${qs.toString()}`
        ),
      ]);
      setTitle(c.title);
      setMetrics(m);
      setRecipients(r.payload || []);
      setTotalPages(r.meta?.total_pages || 1);
    },
    [base]
  );

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    (async () => {
      await loadList();
      if (!active) return;
      if (isAnalytics && campaignId) {
        await loadAnalytics(campaignId, page, statusFilter);
      } else if (isEdit && campaignId) {
        const c = await api<Campaign>(`${base}/campaigns/${campaignId}`);
        if (!active) return;
        setTitle(c.title || "");
        setMessage(c.message || "");
        setInboxId(c.inbox?.id ? String(c.inbox.id) : "");
        setTemplateName(String(c.template_params?.name || ""));
        setTemplateLang(String(c.template_params?.language || "en"));
        setSelectedLabelIds((c.audience || []).map((a) => Number(a.id)));
        if (c.scheduled_at) {
          setScheduledAt(new Date(c.scheduled_at * 1000).toISOString().slice(0, 16));
        }
      }
    })()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load WhatsApp campaigns");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [base, campaignId, isAnalytics, isEdit, loadAnalytics, loadList, page, statusFilter]);

  const activeInbox = waInboxes.find((i) => String(i.id) === inboxId);
  const templates = activeInbox?.message_templates || [];

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim() || !inboxId || !message.trim()) return;
    setBusy(true);
    setError("");
    try {
      const payload = {
        campaign: {
          title: title.trim(),
          inbox_id: Number(inboxId),
          message: message.trim(),
          campaign_type: "one_off",
          enabled: true,
          scheduled_at: scheduledAt ? Math.floor(new Date(scheduledAt).getTime() / 1000) : Math.floor(Date.now() / 1000),
          audience: selectedLabelIds.map((id) => ({ type: "Label", id })),
          template_params: templateName
            ? { name: templateName, language: templateLang || "en", parameters: [] }
            : null,
        },
      };
      if (isEdit && campaignId) {
        await api(`${base}/campaigns/${campaignId}`, { method: "PATCH", json: payload });
      } else {
        await api(`${base}/campaigns`, { method: "POST", json: payload });
      }
      navigate(`/accounts/${accountId}/campaigns/whatsapp`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save WhatsApp campaign");
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete(id: number) {
    setBusy(true);
    setError("");
    try {
      await api(`${base}/campaigns/${id}`, { method: "DELETE" });
      await loadList();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Delete failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-1 flex-col overflow-y-auto bg-n-surface-1 p-6 sm:p-8 text-n-slate-12">
      <CampaignsTabs accountId={accountId} />
      <header className="mb-6 flex flex-wrap items-center justify-between gap-4 border-b border-n-weak pb-4">
        <div className="flex items-center gap-3">
          {(isNew || isEdit || isAnalytics) && (
            <Link
              to={`/accounts/${accountId}/campaigns/whatsapp`}
              className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12"
            >
              <ArrowLeft className="size-4" />
            </Link>
          )}
          <Megaphone className="size-5 text-n-teal-11" />
          <h1 className="text-xl font-medium">
            {isAnalytics ? `${title} — Analytics` : isEdit ? "Edit WhatsApp Campaign" : isNew ? "New WhatsApp Campaign" : "WhatsApp Campaigns"}
          </h1>
        </div>
        {!isNew && !isEdit && !isAnalytics && isAdmin && (
          <Link to={`/accounts/${accountId}/campaigns/whatsapp/new`}>
            <Button solid blue sm label="Create Campaign">
              <Plus className="size-4" />
            </Button>
          </Link>
        )}
      </header>

      {error && (
        <div role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">
          {error}
        </div>
      )}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading...</p>
      ) : isAnalytics ? (
        <div className="space-y-6 max-w-5xl">
          {metrics && (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
              {[
                ["Audience", metrics.audience],
                ["Sent", metrics.sent],
                ["Delivered", metrics.delivered],
                ["Read", metrics.read],
                ["Failed", metrics.failed],
                ["Skipped", metrics.skipped],
              ].map(([label, val]) => (
                <div key={String(label)} className="rounded-xl border border-n-weak bg-n-solid-1 p-4">
                  <div className="text-xs text-n-slate-11">{label}</div>
                  <div className="mt-1 text-2xl font-semibold">{val}</div>
                </div>
              ))}
            </div>
          )}

          <div className="flex flex-wrap items-center gap-2">
            {["", "sent", "delivered", "read", "failed", "skipped"].map((s) => (
              <button
                key={s || "all"}
                type="button"
                onClick={() => {
                  setStatusFilter(s);
                  setPage(1);
                }}
                className={`rounded-lg px-3 py-1.5 text-xs capitalize ${
                  statusFilter === s ? "bg-n-brand text-white" : "border border-n-weak bg-n-solid-1 text-n-slate-11"
                }`}
              >
                {s || "all"}
              </button>
            ))}
          </div>

          <div className="overflow-x-auto rounded-xl border border-n-weak bg-n-solid-1">
            <table className="w-full text-left text-sm">
              <thead className="border-b border-n-weak bg-n-solid-2 text-xs uppercase text-n-slate-11">
                <tr>
                  <th className="px-4 py-3">Contact</th>
                  <th className="px-4 py-3">Phone</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Error</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-n-weak">
                {recipients.length === 0 ? (
                  <tr>
                    <td colSpan={4} className="px-4 py-8 text-center text-n-slate-11">
                      No recipients recorded yet.
                    </td>
                  </tr>
                ) : (
                  recipients.map((r, idx) => (
                    <tr key={`${r.contact.id}-${idx}`}>
                      <td className="px-4 py-3 font-medium">{r.contact.name || `#${r.contact.id}`}</td>
                      <td className="px-4 py-3 font-mono text-xs">{r.contact.phone_number || "—"}</td>
                      <td className="px-4 py-3 capitalize">{r.status}</td>
                      <td className="px-4 py-3 text-xs text-n-ruby-11">
                        {r.error_message || r.error_title || r.error_code || "—"}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
          {totalPages > 1 && (
            <div className="flex items-center justify-end gap-2">
              <Button outline slate sm disabled={page <= 1} onClick={() => setPage((p) => p - 1)} label="Prev" />
              <span className="text-xs text-n-slate-11">Page {page} of {totalPages}</span>
              <Button outline slate sm disabled={page >= totalPages} onClick={() => setPage((p) => p + 1)} label="Next" />
            </div>
          )}
        </div>
      ) : isNew || isEdit ? (
        <form onSubmit={handleSave} className="max-w-2xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <label className="flex flex-col gap-1 text-sm">
            <span>Campaign Title *</span>
            <input
              required
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Black Friday Promo"
              className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span>WhatsApp Inbox *</span>
            <select
              required
              value={inboxId}
              onChange={(e) => setInboxId(e.target.value)}
              className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
            >
              <option value="">Select WhatsApp inbox</option>
              {waInboxes.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name}
                </option>
              ))}
            </select>
          </label>
          {templates.length > 0 && (
            <label className="flex flex-col gap-1 text-sm">
              <span>Approved Template</span>
              <select
                value={templateName}
                onChange={(e) => {
                  const chosen = templates.find((t) => t.name === e.target.value);
                  setTemplateName(e.target.value);
                  if (chosen?.language) setTemplateLang(chosen.language);
                }}
                className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
              >
                <option value="">None (plain message text)</option>
                {templates.map((t) => (
                  <option key={`${t.name}-${t.language}`} value={t.name}>
                    {t.name} ({t.language})
                  </option>
                ))}
              </select>
            </label>
          )}
          <label className="flex flex-col gap-1 text-sm">
            <span>Message *</span>
            <textarea
              required
              rows={4}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              className="rounded-lg border border-n-weak bg-n-background p-3"
            />
          </label>
          <div className="flex flex-col gap-2 text-sm">
            <span>Audience Labels</span>
            <div className="flex flex-wrap gap-2">
              {labels.map((l) => {
                const active = selectedLabelIds.includes(l.id);
                return (
                  <button
                    key={l.id}
                    type="button"
                    onClick={() =>
                      setSelectedLabelIds((prev) =>
                        active ? prev.filter((x) => x !== l.id) : [...prev, l.id]
                      )
                    }
                    className={`rounded-lg border px-2.5 py-1 text-xs ${
                      active ? "border-n-brand bg-n-blue-2 text-n-blue-11" : "border-n-weak bg-n-background"
                    }`}
                  >
                    {l.title}
                  </button>
                );
              })}
            </div>
          </div>
          <label className="flex flex-col gap-1 text-sm">
            <span>Scheduled Time</span>
            <input
              type="datetime-local"
              value={scheduledAt}
              onChange={(e) => setScheduledAt(e.target.value)}
              className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
            />
          </label>
          <div className="flex justify-end gap-2">
            <Button solid blue type="submit" isLoading={busy} label={isEdit ? "Update Campaign" : "Schedule Campaign"} />
          </div>
        </form>
      ) : campaigns.length === 0 ? (
        <div className="rounded-xl border border-n-weak bg-n-solid-1 p-8 text-center">
          <h2 className="text-base font-medium">No WhatsApp campaigns yet</h2>
          <p className="mt-1 text-sm text-n-slate-11">
            Create a scheduled WhatsApp template broadcast to tagged contacts.
          </p>
        </div>
      ) : (
        <div className="grid max-w-4xl grid-cols-1 gap-4">
          {campaigns.map((c) => (
            <div key={c.id} className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-n-weak bg-n-solid-1 p-4">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium">{c.title}</span>
                  <span className="rounded bg-n-alpha-2 px-2 py-0.5 text-xs capitalize">
                    {c.campaign_status}
                  </span>
                </div>
                <p className="mt-1 truncate text-sm text-n-slate-11">{c.message}</p>
                {c.inbox && <p className="mt-1 text-xs text-n-slate-10">{c.inbox.name}</p>}
              </div>
              <div className="flex items-center gap-2">
                <Link to={`/accounts/${accountId}/campaigns/whatsapp/${c.id}/analytics`}>
                  <Button outline slate sm label="Analytics">
                    <BarChart3 className="size-3.5" />
                  </Button>
                </Link>
                {isAdmin && (
                  <>
                    <Link to={`/accounts/${accountId}/campaigns/whatsapp/${c.id}`}>
                      <Button outline slate sm label="Edit" />
                    </Link>
                    <Button ghost ruby sm onClick={() => handleDelete(c.id)} label="">
                      <Trash2 className="size-4" />
                    </Button>
                  </>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
