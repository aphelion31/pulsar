import { Plus, RefreshCw, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { Button, Empty } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";
import { CaptainNav } from "./nav";

type Assistant = { id: number; name: string };
type Doc = {
  id: number;
  name: string | null;
  external_link: string;
  assistant?: { id: number; name: string };
  status: string;
  sync_status: string | null;
};

export default function CaptainDocumentsRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [docs, setDocs] = useState<Doc[]>([]);
  const [assistants, setAssistants] = useState<Assistant[]>([]);
  const [assistantId, setAssistantId] = useState("");
  const [name, setName] = useState("");
  const [externalLink, setExternalLink] = useState("");
  const [content, setContent] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function load() {
    setError("");
    try {
      const [dRes, aRes] = await Promise.all([
        api<{ payload: Doc[] }>(`${acct(accountId)}/captain/documents`),
        api<{ payload: Assistant[] }>(`${acct(accountId)}/captain/assistants`),
      ]);
      setDocs(dRes.payload ?? []);
      const aList = aRes.payload ?? [];
      setAssistants(aList);
      if (!assistantId && aList[0]) setAssistantId(String(aList[0].id));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load documents");
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId]);

  async function addDocument(e: React.FormEvent) {
    e.preventDefault();
    if (!assistantId || !externalLink.trim()) return;
    setBusy(true);
    setError("");
    try {
      await api(`${acct(accountId)}/captain/documents`, {
        method: "POST",
        json: {
          document: {
            assistant_id: Number(assistantId),
            name: name.trim() || undefined,
            external_link: externalLink.trim(),
            content: content.trim() || undefined,
          },
        },
      });
      setName("");
      setExternalLink("");
      setContent("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not create document");
    } finally {
      setBusy(false);
    }
  }

  async function syncDoc(id: number) {
    await api(`${acct(accountId)}/captain/documents/${id}/sync`, { method: "POST" });
    await load();
  }

  async function deleteDoc(id: number) {
    await api(`${acct(accountId)}/captain/documents/${id}`, { method: "DELETE" });
    await load();
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg">
      <CaptainNav accountId={accountId} active="documents" />
      <div className="mx-auto w-full max-w-6xl space-y-6 p-6">
        <form onSubmit={addDocument} className="space-y-3 rounded-xl border border-line bg-surface p-5">
          <h2 className="text-sm font-semibold text-ink">Add Knowledge Document</h2>
          <div className="grid gap-3 md:grid-cols-3">
            <select
              value={assistantId}
              onChange={(e) => setAssistantId(e.target.value)}
              className="h-10 rounded-lg border border-line bg-bg px-3 text-sm"
            >
              {assistants.length === 0 && <option value="">Create an assistant first</option>}
              {assistants.map((a) => (
                <option key={a.id} value={a.id}>{a.name}</option>
              ))}
            </select>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Document title (optional)"
              className="h-10 rounded-lg border border-line bg-bg px-3 text-sm"
            />
            <input
              required
              value={externalLink}
              onChange={(e) => setExternalLink(e.target.value)}
              placeholder="https://docs.example.com/guide"
              className="h-10 rounded-lg border border-line bg-bg px-3 text-sm"
            />
          </div>
          <textarea
            rows={2}
            value={content}
            onChange={(e) => setContent(e.target.value)}
            placeholder="Optional inline content (fetched automatically from URL if omitted)"
            className="w-full rounded-lg border border-line bg-bg p-3 text-sm"
          />
          <div className="flex justify-end">
            <Button type="submit" disabled={busy || !assistantId}><Plus size={14} /> Add Document</Button>
          </div>
        </form>

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        {docs.length === 0 ? (
          <Empty title="No documents indexed"><p className="text-sm text-ink-3">Add URLs or inline documentation for Captain to reference.</p></Empty>
        ) : (
          <div className="overflow-x-auto rounded-xl border border-line bg-surface">
            <table className="w-full text-left text-sm">
              <thead className="border-b border-line bg-raised text-xs uppercase text-ink-3">
                <tr>
                  <th className="px-4 py-3">Title / Link</th>
                  <th className="px-4 py-3">Assistant</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {docs.map((d) => (
                  <tr key={d.id}>
                    <td className="px-4 py-3">
                      <div className="font-medium text-ink">{d.name || d.external_link}</div>
                      <div className="text-xs text-ink-3">{d.external_link}</div>
                    </td>
                    <td className="px-4 py-3 text-ink-2">{d.assistant?.name || "—"}</td>
                    <td className="px-4 py-3 capitalize text-ink-2">{d.sync_status || d.status}</td>
                    <td className="px-4 py-3 text-right space-x-3">
                      <button onClick={() => void syncDoc(d.id)} className="inline-flex items-center gap-1 text-xs text-accent hover:underline">
                        <RefreshCw size={12} /> Sync
                      </button>
                      <button onClick={() => void deleteDoc(d.id)} className="inline-flex items-center gap-1 text-xs text-bad hover:underline">
                        <Trash2 size={12} /> Delete
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
