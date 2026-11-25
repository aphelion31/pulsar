import { Check, Plus, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { Button } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";
import { CaptainNav } from "./nav";

type Assistant = { id: number; name: string };
type FaqItem = {
  id: number;
  question: string;
  answer: string;
  status: string;
  assistant?: { id: number; name: string };
};

export default function CaptainResponsesRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [tab, setTab] = useState<"faqs" | "suggestions">("faqs");
  const [faqs, setFaqs] = useState<FaqItem[]>([]);
  const [suggestions, setSuggestions] = useState<FaqItem[]>([]);
  const [assistants, setAssistants] = useState<Assistant[]>([]);
  const [assistantId, setAssistantId] = useState("");
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState("");
  const [error, setError] = useState("");

  async function load() {
    setError("");
    try {
      const [fRes, sRes, aRes] = await Promise.all([
        api<{ payload: FaqItem[] }>(`${acct(accountId)}/captain/assistant_responses`),
        api<{ payload: FaqItem[] }>(`${acct(accountId)}/captain/faq_suggestions`),
        api<{ payload: Assistant[] }>(`${acct(accountId)}/captain/assistants`),
      ]);
      setFaqs(fRes.payload ?? []);
      setSuggestions(sRes.payload ?? []);
      const aList = aRes.payload ?? [];
      setAssistants(aList);
      if (!assistantId && aList[0]) setAssistantId(String(aList[0].id));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load FAQs");
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId]);

  async function addFaq(e: React.FormEvent) {
    e.preventDefault();
    if (!assistantId || !question.trim() || !answer.trim()) return;
    setError("");
    try {
      await api(`${acct(accountId)}/captain/assistant_responses`, {
        method: "POST",
        json: { assistant_response: { assistant_id: Number(assistantId), question: question.trim(), answer: answer.trim() } },
      });
      setQuestion("");
      setAnswer("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not add FAQ");
    }
  }

  async function deleteFaq(id: number) {
    await api(`${acct(accountId)}/captain/assistant_responses/${id}`, { method: "DELETE" });
    await load();
  }

  async function decideSuggestion(id: number, action: "approve" | "dismiss") {
    await api(`${acct(accountId)}/captain/faq_suggestions/${id}/${action}`, { method: "POST" });
    await load();
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-bg">
      <CaptainNav accountId={accountId} active="responses" />
      <div className="mx-auto w-full max-w-6xl space-y-6 p-6">
        <div className="flex gap-2">
          <button
            onClick={() => setTab("faqs")}
            className={`rounded-lg px-3 py-1.5 text-sm ${tab === "faqs" ? "bg-surface font-medium text-ink border border-line" : "text-ink-3"}`}
          >
            FAQs ({faqs.length})
          </button>
          <button
            onClick={() => setTab("suggestions")}
            className={`rounded-lg px-3 py-1.5 text-sm ${tab === "suggestions" ? "bg-surface font-medium text-ink border border-line" : "text-ink-3"}`}
          >
            Suggestions ({suggestions.filter((s) => s.status === "open").length})
          </button>
        </div>

        {error && <p className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-2 text-sm text-bad">{error}</p>}

        {tab === "faqs" ? (
          <div className="space-y-5">
            <form onSubmit={addFaq} className="space-y-3 rounded-xl border border-line bg-surface p-5">
              <div className="grid gap-3 md:grid-cols-2">
                <select value={assistantId} onChange={(e) => setAssistantId(e.target.value)} className="h-10 rounded-lg border border-line bg-bg px-3 text-sm">
                  {assistants.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
                </select>
                <input required value={question} onChange={(e) => setQuestion(e.target.value)} placeholder="Question" className="h-10 rounded-lg border border-line bg-bg px-3 text-sm" />
              </div>
              <textarea required rows={2} value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="Answer" className="w-full rounded-lg border border-line bg-bg p-3 text-sm" />
              <div className="flex justify-end">
                <Button type="submit" disabled={!assistantId}><Plus size={14} /> Add FAQ</Button>
              </div>
            </form>
            <div className="space-y-3">
              {faqs.map((f) => (
                <div key={f.id} className="flex items-start justify-between gap-4 rounded-xl border border-line bg-surface p-4">
                  <div className="space-y-1">
                    <div className="font-medium text-ink">{f.question}</div>
                    <p className="text-sm text-ink-2">{f.answer}</p>
                    <div className="text-xs text-ink-3 capitalize">{f.assistant?.name} · {f.status}</div>
                  </div>
                  <button onClick={() => void deleteFaq(f.id)} className="text-ink-3 hover:text-bad"><Trash2 size={15} /></button>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            {suggestions.map((s) => (
              <div key={s.id} className="flex items-start justify-between gap-4 rounded-xl border border-line bg-surface p-4">
                <div className="space-y-1">
                  <div className="font-medium text-ink">{s.question}</div>
                  <p className="text-sm text-ink-2">{s.answer}</p>
                  <div className="text-xs text-ink-3 capitalize">{s.assistant?.name} · {s.status}</div>
                </div>
                {s.status === "open" && (
                  <div className="flex items-center gap-2">
                    <Button variant="quiet" onClick={() => void decideSuggestion(s.id, "approve")}><Check size={14} /> Approve</Button>
                    <Button variant="quiet" onClick={() => void decideSuggestion(s.id, "dismiss")}><X size={14} /> Dismiss</Button>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
