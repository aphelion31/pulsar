import { useEffect, useState } from "react";
import { Button } from "~/components/ui";
import { api } from "~/lib/api";

const CATEGORIES = [
  "general",
  "captain",
  "facebook",
  "instagram",
  "whatsapp_embedded",
  "tiktok",
  "slack",
  "linear",
  "stripe",
  "notion",
  "shopify",
  "google",
  "microsoft",
  "email",
];

type InstConfig = { id: number; name: string; value: unknown; locked: boolean };

export default function SuperAdminConfigsRoute() {
  const [category, setCategory] = useState("general");
  const [keys, setKeys] = useState<string[]>([]);
  const [values, setValues] = useState<Record<string, string>>({});
  const [allConfigs, setAllConfigs] = useState<InstConfig[]>([]);
  const [savedMsg, setSavedMsg] = useState("");
  const [error, setError] = useState("");

  async function loadCategory(cat: string) {
    setError("");
    try {
      const [appCfg, rawCfg] = await Promise.all([
        api<{ keys: string[]; values: Record<string, unknown> }>(`/super_admin/api/app_config?config=${cat}`),
        api<{ payload: InstConfig[] }>("/super_admin/api/installation_configs"),
      ]);
      setKeys(appCfg.keys ?? []);
      const mapped: Record<string, string> = {};
      for (const k of appCfg.keys ?? []) {
        const v = appCfg.values?.[k];
        mapped[k] = v == null ? "" : String(v);
      }
      setValues(mapped);
      setAllConfigs(rawCfg.payload ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load app config");
    }
  }

  useEffect(() => {
    void loadCategory(category);
  }, [category]);

  async function saveCategory(e: React.FormEvent) {
    e.preventDefault();
    setSavedMsg("");
    setError("");
    try {
      await api(`/super_admin/api/app_config?config=${category}`, {
        method: "POST",
        json: { app_config: values },
      });
      setSavedMsg(`Saved ${category} settings.`);
      await loadCategory(category);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save settings");
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold text-ink">App & Installation Configs</h1>
        <select
          aria-label="Config Category"
          value={category}
          onChange={(e) => setCategory(e.target.value)}
          className="h-9 rounded-lg border border-line bg-surface px-3 text-sm capitalize text-ink"
        >
          {CATEGORIES.map((c) => <option key={c} value={c}>{c.replace("_", " ")}</option>)}
        </select>
      </div>

      {error && <p className="text-sm text-bad">{error}</p>}
      {savedMsg && <p className="rounded-lg border border-line bg-surface px-3 py-2 text-sm text-ink">{savedMsg}</p>}

      <form onSubmit={saveCategory} className="space-y-4 rounded-xl border border-line bg-surface p-5">
        {keys.map((k) => (
          <label key={k} className="flex flex-col gap-1 text-xs font-medium text-ink-2">
            {k}
            <input
              value={values[k] ?? ""}
              onChange={(e) => setValues({ ...values, [k]: e.target.value })}
              className="h-9 rounded-lg border border-line bg-bg px-3 font-mono text-sm text-ink"
            />
          </label>
        ))}
        <div className="flex justify-end">
          <Button type="submit">Save {category} Config</Button>
        </div>
      </form>

      <div className="overflow-x-auto rounded-xl border border-line bg-surface">
        <table className="w-full text-left text-sm">
          <thead className="border-b border-line bg-raised text-xs uppercase text-ink-3">
            <tr>
              <th className="px-4 py-3">Key</th>
              <th className="px-4 py-3">Value</th>
              <th className="px-4 py-3">Locked</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {allConfigs.map((c) => (
              <tr key={c.id}>
                <td className="px-4 py-2.5 font-mono text-xs text-ink">{c.name}</td>
                <td className="px-4 py-2.5 font-mono text-xs text-ink-2">{JSON.stringify(c.value)}</td>
                <td className="px-4 py-2.5 text-xs text-ink-3">{c.locked ? "Yes" : "No"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
