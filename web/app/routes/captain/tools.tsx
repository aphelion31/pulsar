import { Play, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useOutletContext, useParams } from "react-router";
import { Button } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";
import { CaptainNav } from "./nav";

type Assistant = { id: number; name: string };
type CustomTool = {
  id: number;
  title: string;
  slug: string;
  http_method: string;
  endpoint_url: string;
  enabled: boolean;
};
type McpServer = {
  id: number;
  name: string;
  url: string;
  status: string;
  enabled: boolean;
  tools: { name: string }[];
};

export default function CaptainToolsRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { assistantId: paramAid } = useParams();
  const [assistants, setAssistants] = useState<Assistant[]>([]);
  const [assistantId, setAssistantId] = useState(paramAid || "");
  const [customTools, setCustomTools] = useState<CustomTool[]>([]);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [title, setTitle] = useState("");
  const [method, setMethod] = useState("GET");
  const [endpointUrl, setEndpointUrl] = useState("");
  const [testResult, setTestResult] = useState("");
  const [mcpName, setMcpName] = useState("");
  const [mcpUrl, setMcpUrl] = useState("");
  const [error, setError] = useState("");

  async function load() {
    setError("");
    try {
      const aRes = await api<{ payload: Assistant[] }>(`${acct(accountId)}/captain/assistants`);
      const aList = aRes.payload ?? [];
      setAssistants(aList);
      const activeAid = assistantId || paramAid || (aList[0] ? String(aList[0].id) : "");
      if (!assistantId && activeAid) setAssistantId(activeAid);
      const q = activeAid ? `?assistant_id=${activeAid}` : "";
      const [ctRes, mcpRes] = await Promise.all([
        api<{ payload: CustomTool[] }>(`${acct(accountId)}/captain/custom_tools${q}`),
        api<{ payload: McpServer[] }>(`${acct(accountId)}/captain/mcp_servers${q}`),
      ]);
      setCustomTools(ctRes.payload ?? []);
      setMcpServers(mcpRes.payload ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load tools");
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId, assistantId]);

  async function createCustomTool(e: React.FormEvent) {
    e.preventDefault();
    if (!assistantId || !title.trim() || !endpointUrl.trim()) return;
    setError("");
    try {
      await api(`${acct(accountId)}/captain/custom_tools`, {
        method: "POST",
        json: {
          custom_tool: {
            assistant_id: Number(assistantId),
            title: title.trim(),
            http_method: method,
            endpoint_url: endpointUrl.trim(),
          },
        },
      });
      setTitle("");
      setEndpointUrl("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create tool");
    }
  }

  async function testDraftTool() {
    if (!endpointUrl.trim()) return;
    setTestResult("");
    setError("");
    try {
      const res = await api<{ status: number; body: string }>(`${acct(accountId)}/captain/custom_tools/test`, {
        method: "POST",
        json: { custom_tool: { http_method: method, endpoint_url: endpointUrl.trim() } },
      });
      setTestResult(`HTTP ${res.status}: ${res.body}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Tool test failed");
    }
  }

  async function createMcpServer(e: React.FormEvent) {
    e.preventDefault();
    if (!assistantId || !mcpName.trim() || !mcpUrl.trim()) return;
    setError("");
    try {
      await api(`${acct(accountId)}/captain/mcp_servers`, {
        method: "POST",
        json: { mcp_server: { assistant_id: Number(assistantId), name: mcpName.trim(), url: mcpUrl.trim() } },
      });
      setMcpName("");
      setMcpUrl("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add MCP server");
    }
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg">
      <CaptainNav accountId={accountId} active="tools" />
      <div className="mx-auto w-full max-w-6xl space-y-6 p-6">
        <div className="flex items-center gap-3">
          <span className="text-sm font-medium text-ink">Assistant:</span>
          <select value={assistantId} onChange={(e) => setAssistantId(e.target.value)} className="h-9 rounded-lg border border-line bg-surface px-3 text-sm">
            {assistants.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
          </select>
        </div>

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        <div className="grid gap-6 lg:grid-cols-2">
          <div className="space-y-4">
            <form onSubmit={createCustomTool} className="space-y-3 rounded-xl border border-line bg-surface p-5">
              <h2 className="text-sm font-semibold text-ink">Custom HTTP Tools</h2>
              <input required value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Order Lookup" className="h-10 w-full rounded-lg border border-line bg-bg px-3 text-sm" />
              <div className="flex gap-2">
                <select value={method} onChange={(e) => setMethod(e.target.value)} className="h-10 rounded-lg border border-line bg-bg px-2 text-sm">
                  {["GET", "POST", "PUT", "PATCH", "DELETE"].map((m) => <option key={m} value={m}>{m}</option>)}
                </select>
                <input required value={endpointUrl} onChange={(e) => setEndpointUrl(e.target.value)} placeholder="https://api.example.com/orders" className="h-10 flex-1 rounded-lg border border-line bg-bg px-3 text-sm" />
              </div>
              {testResult && <pre className="overflow-x-auto rounded-lg bg-raised p-2 text-xs text-ink-2">{testResult}</pre>}
              <div className="flex justify-end gap-2">
                <Button type="button" variant="quiet" onClick={() => void testDraftTool()} disabled={!endpointUrl.trim()}>
                  <Play size={14} /> Test
                </Button>
                <Button type="submit" disabled={!assistantId}><Plus size={14} /> Save Tool</Button>
              </div>
            </form>
            <div className="space-y-2">
              {customTools.map((t) => (
                <div key={t.id} className="flex items-center justify-between rounded-xl border border-line bg-surface p-4">
                  <div>
                    <div className="font-medium text-ink">{t.title}</div>
                    <div className="font-mono text-xs text-ink-3">{t.http_method} {t.endpoint_url}</div>
                  </div>
                  <button
                    onClick={async () => {
                      await api(`${acct(accountId)}/captain/custom_tools/${t.id}`, { method: "DELETE" });
                      await load();
                    }}
                    className="text-ink-3 hover:text-bad"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              ))}
            </div>
          </div>

          <div className="space-y-4">
            <form onSubmit={createMcpServer} className="space-y-3 rounded-xl border border-line bg-surface p-5">
              <h2 className="text-sm font-semibold text-ink">MCP Servers</h2>
              <input required value={mcpName} onChange={(e) => setMcpName(e.target.value)} placeholder="Server name" className="h-10 w-full rounded-lg border border-line bg-bg px-3 text-sm" />
              <input required value={mcpUrl} onChange={(e) => setMcpUrl(e.target.value)} placeholder="https://mcp.example.com/sse" className="h-10 w-full rounded-lg border border-line bg-bg px-3 text-sm" />
              <div className="flex justify-end">
                <Button type="submit" disabled={!assistantId}><Plus size={14} /> Connect Server</Button>
              </div>
            </form>
            <div className="space-y-2">
              {mcpServers.map((s) => (
                <div key={s.id} className="flex items-center justify-between rounded-xl border border-line bg-surface p-4">
                  <div>
                    <div className="font-medium text-ink">{s.name}</div>
                    <div className="text-xs text-ink-3">{s.url} · {s.tools?.length ?? 0} tools</div>
                  </div>
                  <button
                    onClick={async () => {
                      await api(`${acct(accountId)}/captain/mcp_servers/${s.id}`, { method: "DELETE" });
                      await load();
                    }}
                    className="text-ink-3 hover:text-bad"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
