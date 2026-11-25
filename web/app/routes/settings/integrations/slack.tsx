import { ArrowLeft } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../../shell";

type Hook = {
  id: number;
  reference_id?: string | null;
  status?: boolean | string | number;
  settings?: { channel_name?: string; message_mode?: string };
};

type SlackApp = { id: string; name: string; description?: string; hooks?: Hook[] };
type Channel = { id: string; name: string; is_private?: boolean };

export default function SlackIntegrationPage() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = acct(accountId);
  const [app, setApp] = useState<SlackApp | null>(null);
  const [channels, setChannels] = useState<Channel[]>([]);
  const [selectedChannel, setSelectedChannel] = useState("");
  const [mode, setMode] = useState("alert");
  const [code, setCode] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    const res = await api<SlackApp>(`${base}/integrations/apps/slack`);
    setApp(res);
    const hook = res.hooks?.[0];
    if (hook) {
      setSelectedChannel(hook.reference_id || "");
      setMode(hook.settings?.message_mode || "alert");
      try {
        const chs = await api<Channel[]>(`${base}/integrations/slack/list_all_channels`);
        setChannels(chs);
      } catch {
        setChannels([]);
      }
    }
  }, [base]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    load()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load Slack");
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
    if (!code.trim()) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/slack`, { method: "POST", json: { code: code.trim() } });
      setCode("");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to connect Slack");
    } finally {
      setBusy(false);
    }
  }

  async function handleUpdateChannel() {
    if (!selectedChannel) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/slack`, { method: "PATCH", json: { reference_id: selectedChannel } });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to update Slack channel");
    } finally {
      setBusy(false);
    }
  }

  async function handleModeChange(nextMode: string) {
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/slack`, { method: "PATCH", json: { message_mode: nextMode } });
      setMode(nextMode);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to update mode");
    } finally {
      setBusy(false);
    }
  }

  async function handleDisconnect() {
    setBusy(true);
    setError("");
    try {
      await api(`${base}/integrations/slack`, { method: "DELETE" });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to disconnect Slack");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 sm:p-8 text-n-slate-12">
      <Link to={`/accounts/${accountId}/settings/integrations`} className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12 mb-4">
        <ArrowLeft className="size-4" /> Integrations
      </Link>
      <h1 className="text-xl font-medium mb-2">{app?.name || "Slack"}</h1>
      {app?.description && <p className="text-sm text-n-slate-11 mb-6">{app.description}</p>}
      {error && <p role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">{error}</p>}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading...</p>
      ) : hook ? (
        <div className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <div className="text-sm">
            Connected channel: <span className="font-medium">{hook.settings?.channel_name || hook.reference_id || "None"}</span>
          </div>
          {channels.length > 0 && (
            <div className="flex gap-2">
              <select value={selectedChannel} onChange={(e) => setSelectedChannel(e.target.value)} className="h-10 flex-1 rounded-lg border border-n-weak bg-n-background px-3 text-sm">
                <option value="">Select Slack channel</option>
                {channels.map((c) => (
                  <option key={c.id} value={c.id}>#{c.name}</option>
                ))}
              </select>
              <Button solid blue isLoading={busy} onClick={handleUpdateChannel} label="Update Channel" />
            </div>
          )}
          <label className="flex flex-col gap-1 text-sm">
            <span>Message Mode</span>
            <select value={mode} onChange={(e) => handleModeChange(e.target.value)} className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm">
              <option value="alert">Alerts only</option>
              <option value="two_way">Two-way sync</option>
            </select>
          </label>
          <div className="pt-2">
            <Button solid ruby isLoading={busy} onClick={handleDisconnect} label="Disconnect Slack" />
          </div>
        </div>
      ) : (
        <form onSubmit={handleConnect} className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <label className="flex flex-col gap-1 text-sm">
            <span>Slack OAuth Authorization Code</span>
            <input required value={code} onChange={(e) => setCode(e.target.value)} placeholder="Paste OAuth code" className="h-10 rounded-lg border border-n-weak bg-n-background px-3 font-mono text-xs" />
          </label>
          <Button solid blue type="submit" isLoading={busy} label="Connect Slack" />
        </form>
      )}
    </div>
  );
}
