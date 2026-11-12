import { useEffect, useState } from "react";
import { Link } from "react-router";
import { api } from "~/lib/api";

type Row = { id: number; name?: string; title?: string; conversations_count?: number; resolved_conversations_count?: number; avg_first_response_time?: number; avg_resolution_time?: number };
export default function EntityReport({ accountId, kind, id }: { accountId: number; kind: "agent" | "inbox" | "label" | "team"; id?: string }) {
  const [rows, setRows] = useState<Row[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    api<Row[]>(`/api/v2/accounts/${accountId}/summary_reports/${kind}`).then(data => { if (active) setRows(data); }).catch((err: unknown) => { if (active) setError(err instanceof Error ? err.message : "Failed to load reports"); });
    return () => { active = false; };
  }, [accountId, kind]);
  const selected = id ? rows.filter(row => String(row.id) === id) : rows;
  const plural = kind === "inbox" ? "inboxes" : `${kind}s`;
  return <div className="h-full overflow-auto p-6"><h1 className="mb-6 text-xl font-semibold capitalize">{kind} {id ? "report" : "overview"}</h1>{error && <p role="alert">{error}</p>}<table className="w-full text-left text-sm"><thead><tr><th>Name</th><th>Conversations</th><th>Resolutions</th><th>First response time</th><th>Resolution time</th></tr></thead><tbody>{selected.map(row => <tr key={row.id} className="border-t border-slate-200"><td className="py-3"><Link className="text-blue-700 underline" to={`/accounts/${accountId}/reports/${plural}/${row.id}`}>{row.name ?? row.title ?? row.id}</Link></td><td>{row.conversations_count ?? 0}</td><td>{row.resolved_conversations_count ?? 0}</td><td>{Math.round(row.avg_first_response_time ?? 0)} s</td><td>{Math.round(row.avg_resolution_time ?? 0)} s</td></tr>)}</tbody></table>{!error && selected.length === 0 && <p className="mt-6 text-sm text-slate-500">No data available</p>}</div>;
}
