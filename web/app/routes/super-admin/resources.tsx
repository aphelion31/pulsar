import { Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "~/components/ui";
import { api } from "~/lib/api";

type Bot = { id: number; name: string | null; outgoing_url: string | null };
type AppRow = { id: number; name: string; access_token: string | null };
type Banner = { id: number; title: string | null; banner_message: string; active: boolean };
type TokenRow = { id: number; owner_type: string | null; owner_id: number | null; token: string | null };

export default function SuperAdminResourcesRoute() {
  const [tab, setTab] = useState<"bots" | "apps" | "banners" | "tokens">("bots");
  const [bots, setBots] = useState<Bot[]>([]);
  const [apps, setApps] = useState<AppRow[]>([]);
  const [banners, setBanners] = useState<Banner[]>([]);
  const [tokens, setTokens] = useState<TokenRow[]>([]);
  const [inputA, setInputA] = useState("");
  const [inputB, setInputB] = useState("");
  const [error, setError] = useState("");

  async function load() {
    setError("");
    try {
      const [b, a, bn, t] = await Promise.all([
        api<{ payload: Bot[] }>("/super_admin/api/agent_bots"),
        api<{ payload: AppRow[] }>("/super_admin/api/platform_apps"),
        api<{ payload: Banner[] }>("/super_admin/api/platform_banners"),
        api<{ payload: TokenRow[] }>("/super_admin/api/access_tokens"),
      ]);
      setBots(b.payload ?? []);
      setApps(a.payload ?? []);
      setBanners(bn.payload ?? []);
      setTokens(t.payload ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load resources");
    }
  }

  useEffect(() => {
    void load();
  }, []);

  async function createItem(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    try {
      if (tab === "bots") {
        await api("/super_admin/api/agent_bots", {
          method: "POST",
          json: { agent_bot: { name: inputA.trim(), outgoing_url: inputB.trim() || null } },
        });
      } else if (tab === "apps") {
        await api("/super_admin/api/platform_apps", {
          method: "POST",
          json: { platform_app: { name: inputA.trim() } },
        });
      } else if (tab === "banners") {
        await api("/super_admin/api/platform_banners", {
          method: "POST",
          json: { platform_banner: { title: inputA.trim() || null, banner_message: inputB.trim(), active: true } },
        });
      }
      setInputA("");
      setInputB("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Create failed");
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold text-ink">Platform Resources</h1>
        <div className="flex gap-1.5">
          {[
            { k: "bots", l: "Agent Bots" },
            { k: "apps", l: "Platform Apps" },
            { k: "banners", l: "Banners" },
            { k: "tokens", l: "Access Tokens" },
          ].map((x) => (
            <button
              key={x.k}
              onClick={() => setTab(x.k as typeof tab)}
              className={`rounded-lg px-3 py-1.5 text-sm ${tab === x.k ? "bg-accent text-accent-ink font-medium" : "bg-surface text-ink-2 border border-line"}`}
            >
              {x.l}
            </button>
          ))}
        </div>
      </div>

      {tab !== "tokens" && (
        <form onSubmit={createItem} className="grid gap-3 rounded-xl border border-line bg-surface p-4 md:grid-cols-3">
          <input
            required={tab !== "banners"}
            value={inputA}
            onChange={(e) => setInputA(e.target.value)}
            placeholder={tab === "bots" ? "Bot name" : tab === "apps" ? "Platform app name" : "Banner title (optional)"}
            className="h-9 rounded-lg border border-line bg-bg px-3 text-sm"
          />
          {tab !== "apps" ? (
            <input
              required={tab === "banners"}
              value={inputB}
              onChange={(e) => setInputB(e.target.value)}
              placeholder={tab === "bots" ? "Outgoing URL" : "Banner message"}
              className="h-9 rounded-lg border border-line bg-bg px-3 text-sm"
            />
          ) : (
            <div />
          )}
          <Button type="submit"><Plus size={14} /> Create</Button>
        </form>
      )}

      {error && <p className="text-sm text-bad">{error}</p>}

      {tab === "bots" && (
        <div className="space-y-2">
          {bots.map((b) => (
            <div key={b.id} className="flex items-center justify-between rounded-xl border border-line bg-surface p-4">
              <div>
                <div className="font-medium text-ink">{b.name}</div>
                <div className="text-xs text-ink-3">{b.outgoing_url || "No outgoing URL"}</div>
              </div>
              <button
                onClick={async () => {
                  await api(`/super_admin/api/agent_bots/${b.id}`, { method: "DELETE" });
                  await load();
                }}
                className="text-bad hover:opacity-80"
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      )}

      {tab === "apps" && (
        <div className="space-y-2">
          {apps.map((a) => (
            <div key={a.id} className="flex items-center justify-between rounded-xl border border-line bg-surface p-4">
              <div>
                <div className="font-medium text-ink">{a.name}</div>
                <div className="font-mono text-xs text-ink-3">Token: {a.access_token || "—"}</div>
              </div>
              <button
                onClick={async () => {
                  await api(`/super_admin/api/platform_apps/${a.id}`, { method: "DELETE" });
                  await load();
                }}
                className="text-bad hover:opacity-80"
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      )}

      {tab === "banners" && (
        <div className="space-y-2">
          {banners.map((b) => (
            <div key={b.id} className="flex items-center justify-between rounded-xl border border-line bg-surface p-4">
              <div>
                <div className="font-medium text-ink">{b.title || "Banner"}</div>
                <div className="text-xs text-ink-2">{b.banner_message}</div>
              </div>
              <button
                onClick={async () => {
                  await api(`/super_admin/api/platform_banners/${b.id}`, { method: "DELETE" });
                  await load();
                }}
                className="text-bad hover:opacity-80"
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      )}

      {tab === "tokens" && (
        <div className="overflow-x-auto rounded-xl border border-line bg-surface">
          <table className="w-full text-left text-sm">
            <thead className="border-b border-line bg-raised text-xs uppercase text-ink-3">
              <tr>
                <th className="px-4 py-3">Owner</th>
                <th className="px-4 py-3">Token</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {tokens.map((t) => (
                <tr key={t.id}>
                  <td className="px-4 py-2.5 text-ink">{t.owner_type} #{t.owner_id}</td>
                  <td className="px-4 py-2.5 font-mono text-xs text-ink-2">{t.token}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
