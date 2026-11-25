import { Store } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import Button from "~/components/next/Button";
import { acct, api, type Profile } from "~/lib/api";

export default function ShopifySelectAccount() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const token = params.get("pending_install_token") || "";
  const [accounts, setAccounts] = useState<Profile["accounts"]>([]);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    api<Profile>("/api/v1/profile")
      .then((p) => {
        if (active) setAccounts(p.accounts || []);
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load accounts");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  async function selectAccount(accountId: number) {
    setBusyId(accountId);
    setError("");
    try {
      await api(`${acct(accountId)}/integrations/shopify/complete_install`, {
        method: "POST",
        json: { pending_install_token: token },
      });
      navigate(`/accounts/${accountId}/settings/integrations/shopify`, { replace: true });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to connect store");
    } finally {
      setBusyId(null);
    }
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-n-surface-1 p-6 text-n-slate-12">
      <div className="w-full max-w-md rounded-2xl border border-n-weak bg-n-solid-1 p-6 shadow-sm">
        <div className="mb-4 flex items-center gap-3">
          <Store className="size-6 text-n-teal-11" />
          <h1 className="text-lg font-semibold">Select Account for Shopify</h1>
        </div>
        {error && (
          <p role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">
            {error}
          </p>
        )}
        {loading ? (
          <p className="text-sm text-n-slate-11">Loading accounts...</p>
        ) : accounts.length === 0 ? (
          <p className="text-sm text-n-slate-11">No accounts available.</p>
        ) : (
          <div className="space-y-2">
            {accounts.map((a) => (
              <div
                key={a.id}
                className="flex items-center justify-between rounded-xl border border-n-weak bg-n-background p-3"
              >
                <div>
                  <div className="font-medium text-sm">{a.name}</div>
                  <div className="text-xs text-n-slate-11 capitalize">{a.role}</div>
                </div>
                <Button
                  solid
                  blue
                  sm
                  isLoading={busyId === a.id}
                  onClick={() => selectAccount(a.id)}
                  label="Connect"
                />
              </div>
            ))}
          </div>
        )}
      </div>
    </main>
  );
}
