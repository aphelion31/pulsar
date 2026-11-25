import { ArrowLeft } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../../shell";

type StripeState = {
  account_id: string | null;
  connected_at?: string | null;
  mode?: string;
  reauthorization_required?: boolean;
};

export default function StripeIntegrationPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = acct(accountId);
  const [conn, setConn] = useState<StripeState | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      const s = await api<StripeState>(`${base}/integrations/stripe`);
      setConn(s);
    } catch {
      setConn(null);
    }
  }, [base]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    load().finally(() => {
      if (active) setLoading(false);
    });
    return () => {
      active = false;
    };
  }, [load]);

  async function handleConnect() {
    setBusy(true);
    setError("");
    try {
      const r = await api<{ url: string }>(`${base}/integrations/stripe/auth`, { method: "POST", json: {} });
      window.location.href = r.url;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start Stripe OAuth");
      setBusy(false);
    }
  }

  async function handleDisconnect() {
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/stripe`, { method: "DELETE" });
      setConn(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to disconnect Stripe");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 sm:p-8 text-n-slate-12">
      <Link to={`/accounts/${accountId}/settings/integrations`} className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12 mb-4">
        <ArrowLeft className="size-4" /> Integrations
      </Link>
      <h1 className="text-xl font-medium mb-2">Stripe</h1>
      <p className="text-sm text-n-slate-11 mb-6">Inspect customer subscriptions and recent invoices right inside the conversation sidebar.</p>
      {error && <p role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">{error}</p>}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading...</p>
      ) : conn ? (
        <div className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <div className="text-sm font-medium text-n-teal-11">Stripe connected ({conn.mode})</div>
          <div className="text-xs text-n-slate-11">Account: <span className="font-mono">{conn.account_id}</span></div>
          {conn.connected_at && <div className="text-xs text-n-slate-11">Connected: {conn.connected_at}</div>}
          <Button solid ruby isLoading={busy} onClick={handleDisconnect} label="Disconnect Stripe" />
        </div>
      ) : (
        <div className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <Button solid blue isLoading={busy} onClick={handleConnect} label="Connect Stripe" />
        </div>
      )}
    </div>
  );
}
