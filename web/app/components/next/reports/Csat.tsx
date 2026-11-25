import { useEffect, useState } from "react";
import { Link } from "react-router";
import { api } from "~/lib/api";

type Response = { id: number; rating: number; feedback_message: string | null; conversation_id: number | null; created_at: number };
type Metrics = { total_count: number; ratings_count: Record<string, number>; total_sent_messages_count: number };
export default function Csat({ accountId }: { accountId: number }) {
  const [rows, setRows] = useState<Response[]>([]);
  const [metrics, setMetrics] = useState<Metrics>();
  const [page, setPage] = useState(1);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    const base = `/api/v1/accounts/${accountId}/csat_survey_responses`;
    Promise.all([api<Response[]>(`${base}?page=${page}`), api<Metrics>(`${base}/metrics`)]).then(([r, m]) => { if (active) { setRows(r); setMetrics(m); setError(""); } }).catch((err: unknown) => { if (active) setError(err instanceof Error ? err.message : "Failed to load responses"); });
    return () => { active = false; };
  }, [accountId, page]);
  return <div className="h-full overflow-auto p-6"><h1 className="mb-6 text-xl font-semibold">CSAT Reports</h1>{error && <p role="alert">{error}</p>}<div className="mb-6 flex gap-8"><p>Responses: {metrics?.total_count ?? "—"}</p><p>Surveys sent: {metrics?.total_sent_messages_count ?? "—"}</p></div><table className="w-full text-left text-sm"><thead><tr><th>Rating</th><th>Feedback</th><th>Conversation</th><th>Recorded at</th></tr></thead><tbody>{rows.map(row => <tr key={row.id} className="border-t border-slate-200"><td className="py-3">{row.rating}</td><td>{row.feedback_message || "—"}</td><td>{row.conversation_id ? <Link className="text-blue-700 underline" to={`/accounts/${accountId}/conversations/${row.conversation_id}`}>#{row.conversation_id}</Link> : "—"}</td><td>{new Date(row.created_at * 1000).toLocaleString()}</td></tr>)}</tbody></table>{!rows.length && !error && <p className="mt-6 text-sm">No responses found</p>}<div className="mt-4 flex gap-4"><button disabled={page === 1} onClick={() => setPage(page - 1)}>Previous</button><span>Page {page}</span><button disabled={rows.length < 25} onClick={() => setPage(page + 1)}>Next</button></div></div>;
}
