import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { Button } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";
import { CaptainNav } from "./nav";

type FeaturePref = {
  name: string;
  enabled: boolean;
  model: string;
  default: string;
};

type PreferencesPayload = {
  models: Record<string, { id: string; name: string; provider: string }>;
  features: Record<string, FeaturePref>;
};

export default function CaptainPreferencesRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [prefs, setPrefs] = useState<PreferencesPayload | null>(null);
  const [selectedModels, setSelectedModels] = useState<Record<string, string>>({});
  const [enabledFlags, setEnabledFlags] = useState<Record<string, boolean>>({});
  const [saving, setSaving] = useState(false);
  const [savedMsg, setSavedMsg] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    api<PreferencesPayload>(`${acct(accountId)}/captain/preferences`)
      .then((res) => {
        if (!active) return;
        setPrefs(res);
        const m: Record<string, string> = {};
        const f: Record<string, boolean> = {};
        for (const [k, v] of Object.entries(res.features)) {
          m[k] = v.model || v.default;
          f[k] = v.enabled;
        }
        setSelectedModels(m);
        setEnabledFlags(f);
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load preferences");
      });
    return () => {
      active = false;
    };
  }, [accountId]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setSaving(true);
    setSavedMsg("");
    setError("");
    try {
      const updated = await api<PreferencesPayload>(`${acct(accountId)}/captain/preferences`, {
        method: "PATCH",
        json: {
          captain_models: selectedModels,
          captain_features: enabledFlags,
        },
      });
      setPrefs(updated);
      setSavedMsg("Captain preferences saved.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save preferences");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg">
      <CaptainNav accountId={accountId} active="preferences" />
      <div className="mx-auto w-full max-w-4xl p-6">
        {error && <p className="mb-4 rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}
        {savedMsg && <p className="mb-4 rounded-lg border border-line bg-surface px-4 py-2 text-sm text-ink">{savedMsg}</p>}
        {prefs && (
          <form onSubmit={save} className="space-y-4 rounded-xl border border-line bg-surface p-6">
            {Object.entries(prefs.features).map(([key, feat]) => (
              <div key={key} className="flex flex-wrap items-center justify-between gap-4 border-b border-line pb-4 last:border-0 last:pb-0">
                <label className="flex items-center gap-2.5 text-sm font-medium text-ink">
                  <input
                    type="checkbox"
                    checked={enabledFlags[key] ?? true}
                    onChange={(e) => setEnabledFlags({ ...enabledFlags, [key]: e.target.checked })}
                    className="size-4"
                  />
                  {feat.name}
                </label>
                <select
                  value={selectedModels[key] || feat.default}
                  onChange={(e) => setSelectedModels({ ...selectedModels, [key]: e.target.value })}
                  className="h-9 rounded-lg border border-line bg-bg px-3 text-sm text-ink"
                >
                  {Object.values(prefs.models).map((m) => (
                    <option key={m.id} value={m.id}>{m.name}</option>
                  ))}
                </select>
              </div>
            ))}
            <div className="flex justify-end pt-2">
              <Button type="submit" disabled={saving}>{saving ? "Saving..." : "Save Preferences"}</Button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
