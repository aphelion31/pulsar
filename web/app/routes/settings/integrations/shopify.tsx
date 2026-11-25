import { ArrowLeft } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../../shell";

type AppInfo = { id: string; name: string; description?: string; hooks?: Array<{ id: number; reference_id?: string | null }> };
type CustomApp = { shop_domain: string; client_id: string; enabled: boolean; callback_url: string; webhook_url: string };

export default function ShopifyIntegrationPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = acct(accountId);
  const [app, setApp] = useState<AppInfo | null>(null);
  const [customApp, setCustomApp] = useState<CustomApp | null>(null);
  const [shopDomain, setShopDomain] = useState("");
  const [caShop, setCaShop] = useState("");
  const [caClientId, setCaClientId] = useState("");
  const [caSecret, setCaSecret] = useState("");
  const [caInstallUrl, setCaInstallUrl] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    const [info, caRes] = await Promise.all([
      api<AppInfo>(`${base}/integrations/apps/shopify`),
      api<{ custom_app: CustomApp | null }>(`${base}/integrations/shopify_custom_app`).catch(() => ({ custom_app: null })),
    ]);
    setApp(info);
    setCustomApp(caRes.custom_app);
    if (caRes.custom_app) {
      setCaShop(caRes.custom_app.shop_domain);
      setCaClientId(caRes.custom_app.client_id);
    }
  }, [base]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    load()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load Shopify");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [load]);

  const hook = app?.hooks?.[0];

  async function handleConnect(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const r = await api<{ redirect_url: string }>(`${base}/integrations/shopify/auth`, {
        method: "POST",
        json: { shop_domain: shopDomain.trim() },
      });
      window.location.href = r.redirect_url;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start Shopify OAuth");
      setBusy(false);
    }
  }

  async function handleDisconnect() {
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/shopify`, { method: "DELETE" });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to disconnect Shopify");
    } finally {
      setBusy(false);
    }
  }

  async function saveCustomApp(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      if (customApp) {
        await api(`${base}/integrations/shopify_custom_app`, {
          method: "PATCH",
          json: { client_secret: caSecret || "", install_url: caInstallUrl.trim() || undefined, enabled: true },
        });
      } else {
        await api(`${base}/integrations/shopify_custom_app`, {
          method: "POST",
          json: { shop_domain: caShop.trim(), client_id: caClientId.trim(), client_secret: caSecret.trim(), install_url: caInstallUrl.trim() },
        });
      }
      setCaSecret("");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save custom app");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 sm:p-8 text-n-slate-12 space-y-6">
      <div>
        <Link to={`/accounts/${accountId}/settings/integrations`} className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12 mb-4">
          <ArrowLeft className="size-4" /> Integrations
        </Link>
        <h1 className="text-xl font-medium mb-1">{app?.name || "Shopify"}</h1>
        {app?.description && <p className="text-sm text-n-slate-11">{app.description}</p>}
      </div>
      {error && <p role="alert" className="rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">{error}</p>}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading...</p>
      ) : (
        <div className="grid max-w-4xl grid-cols-1 gap-6 lg:grid-cols-2">
          <div className="space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6 h-fit">
            <h2 className="text-base font-medium">Store Connection</h2>
            {hook ? (
              <>
                <div className="text-sm text-n-teal-11">Connected to <span className="font-mono">{hook.reference_id}</span></div>
                <Button solid ruby isLoading={busy} onClick={handleDisconnect} label="Disconnect Store" />
              </>
            ) : (
              <form onSubmit={handleConnect} className="space-y-3">
                <input required value={shopDomain} onChange={(e) => setShopDomain(e.target.value)} placeholder="my-store.myshopify.com" className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm font-mono" />
                <Button solid blue type="submit" isLoading={busy} label="Connect Shopify" />
              </form>
            )}
          </div>

          <form onSubmit={saveCustomApp} className="space-y-3 rounded-xl border border-n-weak bg-n-solid-1 p-6">
            <h2 className="text-base font-medium">Custom App Configuration</h2>
            <input required disabled={Boolean(customApp)} value={caShop} onChange={(e) => setCaShop(e.target.value)} placeholder="store.myshopify.com" className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm font-mono" />
            <input required disabled={Boolean(customApp)} value={caClientId} onChange={(e) => setCaClientId(e.target.value)} placeholder="Client ID" className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm font-mono" />
            <input required={!customApp} value={caSecret} onChange={(e) => setCaSecret(e.target.value)} placeholder={customApp ? "Client Secret (leave blank to keep)" : "Client Secret"} className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm font-mono" />
            <input required={!customApp} value={caInstallUrl} onChange={(e) => setCaInstallUrl(e.target.value)} placeholder="Install URL" className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm font-mono" />
            {customApp && (
              <div className="space-y-1 rounded-lg border border-n-weak bg-n-background p-3 text-xs font-mono">
                <div>Callback: {customApp.callback_url}</div>
                <div>Webhook: {customApp.webhook_url}</div>
              </div>
            )}
            <div className="flex gap-2">
              <Button solid blue type="submit" isLoading={busy} label={customApp ? "Update Custom App" : "Save Custom App"} />
              {customApp && (
                <Button outline ruby type="button" isLoading={busy} onClick={async () => { await api(`${base}/integrations/shopify_custom_app`, { method: "DELETE" }); setCaShop(""); setCaClientId(""); setCustomApp(null); }} label="Delete" />
              )}
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
