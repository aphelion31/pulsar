import { ArrowRight, Bot, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import { Button, Empty } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";
import { CaptainNav } from "./nav";

type Assistant = {
  id: number;
  name: string;
  description: string | null;
  connected_inboxes_count: number;
  scenarios_count: number;
};

export default function CaptainAssistantsRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [assistants, setAssistants] = useState<Assistant[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function load() {
    setLoading(true);
    setError("");
    try {
      const res = await api<{ payload: Assistant[] }>(`${acct(accountId)}/captain/assistants`);
      setAssistants(res.payload ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load assistants");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId]);

  async function createAssistant(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    setBusy(true);
    setError("");
    try {
      await api(`${acct(accountId)}/captain/assistants`, {
        method: "POST",
        json: { assistant: { name: name.trim(), description: description.trim() || null } },
      });
      setName("");
      setDescription("");
      setShowCreate(false);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not create assistant");
    } finally {
      setBusy(false);
    }
  }

  async function deleteAssistant(id: number) {
    setError("");
    try {
      await api(`${acct(accountId)}/captain/assistants/${id}`, { method: "DELETE" });
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not delete assistant");
    }
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg">
      <CaptainNav accountId={accountId} active="assistants" />
      <div className="mx-auto w-full max-w-6xl space-y-6 p-6">
        <div className="flex items-center justify-between">
          <p className="text-sm text-ink-3">Create AI support assistants backed by your documents, FAQs, scenarios, and tools.</p>
          <Button onClick={() => setShowCreate((v) => !v)}>
            <Plus size={15} /> New Assistant
          </Button>
        </div>

        {showCreate && (
          <form onSubmit={createAssistant} className="space-y-4 rounded-xl border border-line bg-surface p-5">
            <label className="flex flex-col gap-1 text-sm font-medium text-ink">
              Assistant Name
              <input
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Support Bot"
                className="h-10 rounded-lg border border-line bg-bg px-3 text-sm"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm font-medium text-ink">
              Instructions / Description
              <textarea
                rows={3}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Help customers with billing, onboarding, and troubleshooting."
                className="rounded-lg border border-line bg-bg p-3 text-sm"
              />
            </label>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="quiet" onClick={() => setShowCreate(false)}>Cancel</Button>
              <Button type="submit" disabled={busy}>Create</Button>
            </div>
          </form>
        )}

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        {loading ? (
          <div className="rounded-xl border border-line bg-surface p-8 text-center text-sm text-ink-3">Loading assistants...</div>
        ) : assistants.length === 0 ? (
          <Empty title="No Captain assistants yet">
            <p className="text-sm text-ink-3">Create your first assistant to start automating customer replies.</p>
          </Empty>
        ) : (
          <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
            {assistants.map((a) => (
              <div
                key={a.id}
                className="flex flex-col justify-between rounded-xl border border-n-weak bg-n-solid-1 p-5 shadow-2xs transition hover:border-n-strong hover:shadow-sm"
              >
                <div className="space-y-2">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-2.5">
                      <span className="grid size-7 place-items-center rounded-lg bg-n-blue-3 text-n-blue-11">
                        <Bot size={15} />
                      </span>
                      <Link
                        to={`/accounts/${accountId}/captain/${a.id}/overview`}
                        className="font-semibold text-n-slate-12 hover:text-n-brand hover:underline"
                      >
                        {a.name}
                      </Link>
                    </div>
                    <button
                      onClick={() => void deleteAssistant(a.id)}
                      className="text-n-slate-10 hover:text-n-ruby-11"
                      aria-label={`Delete ${a.name}`}
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                  <p className="text-sm text-n-slate-11 line-clamp-3">{a.description || "No description provided."}</p>
                </div>
                <div className="mt-4 flex items-center justify-between border-t border-n-weak pt-3 text-xs text-n-slate-10">
                  <span>{a.connected_inboxes_count} inboxes · {a.scenarios_count} scenarios</span>
                  <Link
                    to={`/accounts/${accountId}/captain/${a.id}/playground`}
                    className="inline-flex items-center gap-1 font-medium text-n-brand hover:underline"
                  >
                    Open <ArrowRight size={13} />
                  </Link>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
