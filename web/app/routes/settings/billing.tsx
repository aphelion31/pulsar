import { CreditCard, FileText } from "lucide-react";
import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";

type LimitsRes = {
  id: number;
  limits: Record<string, { allowed: number; consumed: number }>;
};

type BillingSummary = {
  id: number;
  plan: string;
  billing_currency: string;
  stripe_customer_id?: string | null;
  marked_for_deletion_at?: string | null;
};

type TopupOptions = {
  currency: string;
  options: { credits: number; amount_cents: number }[];
};

export default function BillingSettingsRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [limits, setLimits] = useState<LimitsRes["limits"] | null>(null);
  const [summary, setSummary] = useState<BillingSummary | null>(null);
  const [topups, setTopups] = useState<TopupOptions | null>(null);
  const [dpaUrl, setDpaUrl] = useState<string | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    const base = `/enterprise/api/v1${acct(accountId).replace("/api/v1", "")}`;
    Promise.all([
      api<LimitsRes>(`${base}/limits`),
      api<BillingSummary>(`${base}/billing_summary`),
      api<TopupOptions>(`${base}/topup_options`),
      api<{ dpa_signing_url: string | null }>(`${base}/legal_compliance`),
    ])
      .then(([lim, sum, top, leg]) => {
        if (!active) return;
        setLimits(lim.limits ?? null);
        setSummary(sum);
        setTopups(top);
        setDpaUrl(leg.dpa_signing_url ?? null);
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load billing details");
      });
    return () => {
      active = false;
    };
  }, [accountId]);

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg p-6">
      <div className="mx-auto w-full max-w-4xl space-y-6">
        <div className="flex items-center gap-2.5">
          <CreditCard size={20} className="text-accent" />
          <div>
            <h1 className="text-xl font-semibold text-ink">Plan & Billing</h1>
            <p className="text-sm text-ink-3">Usage limits, plan details, and compliance settings.</p>
          </div>
        </div>

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        {summary && (
          <div className="rounded-xl border border-line bg-surface p-5">
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div>
                <div className="text-xs uppercase text-ink-3">Current Plan</div>
                <div className="text-lg font-semibold text-ink">{summary.plan}</div>
                <div className="text-xs text-ink-3">Currency: {summary.billing_currency.toUpperCase()}</div>
              </div>
              {dpaUrl && (
                <a href={dpaUrl} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 text-sm font-medium text-accent hover:underline">
                  <FileText size={14} /> View DPA
                </a>
              )}
            </div>
          </div>
        )}

        {limits && (
          <div className="grid gap-4 sm:grid-cols-3">
            {Object.entries(limits).map(([key, item]) => (
              <div key={key} className="rounded-xl border border-line bg-surface p-4">
                <div className="text-xs capitalize text-ink-3">{key.replaceAll("_", " ")}</div>
                <div className="mt-1 font-mono text-xl font-semibold text-ink">
                  {item.consumed} / {item.allowed}
                </div>
              </div>
            ))}
          </div>
        )}

        {topups && topups.options.length > 0 && (
          <div className="rounded-xl border border-line bg-surface p-5">
            <h2 className="mb-3 text-sm font-semibold text-ink">Captain Credit Top-up Packages</h2>
            <div className="grid gap-3 sm:grid-cols-3">
              {topups.options.map((opt) => (
                <div key={opt.credits} className="rounded-lg border border-line bg-bg p-3">
                  <div className="font-mono text-base font-semibold text-ink">{opt.credits} credits</div>
                  <div className="text-xs text-ink-3">
                    {(opt.amount_cents / 100).toFixed(2)} {topups.currency.toUpperCase()}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
