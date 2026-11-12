import { useEffect, useState } from "react";
import { api } from "~/lib/api";

type Metrics = Record<string, number>;
export default function Live({ accountId }: { accountId: number }) {
  const [metrics, setMetrics] = useState<Metrics>({});
  const [agents, setAgents] = useState<Metrics[]>([]);
  const [teams, setTeams] = useState<Metrics[]>([]);
  const [error, setError] = useState("");
  // Grouped metrics carry only ids; show names like Chatwoot's AgentTable/TeamTable.
  const [names, setNames] = useState<Record<string, Record<number, string>>>({ assignee_id: {}, team_id: {} });
  useEffect(() => {
    const v1 = `/api/v1/accounts/${accountId}`;
    Promise.all([api<{ id: number; name: string }[]>(`${v1}/agents`), api<{ id: number; name: string }[]>(`${v1}/teams`)])
      .then(([a, t]) => setNames({
        assignee_id: Object.fromEntries(a.map((x) => [x.id, x.name])),
        team_id: Object.fromEntries(t.map((x) => [x.id, x.name])),
      }))
      .catch(() => {});
  }, [accountId]);
  useEffect(() => {
    let active = true;
    const base = `/api/v2/accounts/${accountId}/live_reports`;
    const load = () => Promise.all([api<Metrics>(`${base}/conversation_metrics`), api<Metrics[]>(`${base}/grouped_conversation_metrics?group_by=assignee_id`), api<Metrics[]>(`${base}/grouped_conversation_metrics?group_by=team_id`)]).then(([m, a, t]) => { if (active) { setMetrics(m); setAgents(a); setTeams(t); setError(""); } }).catch((err: unknown) => { if (active) setError(err instanceof Error ? err.message : "Failed to load live reports"); });
    void load();
    const timer = setInterval(load, 60000);
    return () => { active = false; clearInterval(timer); };
  }, [accountId]);
  return <div className="h-full overflow-auto p-6"><h1 className="mb-6 text-xl font-semibold">Overview</h1>{error && <p role="alert">{error}</p>}<div className="grid gap-4 sm:grid-cols-3">{Object.entries(metrics).map(([key, value]) => <section className="rounded-lg border border-slate-200 p-5" key={key}><h2 className="text-sm capitalize">{key.replaceAll("_", " ")}</h2><p className="mt-3 text-2xl font-semibold">{value}</p></section>)}</div>{[["Agents", agents, "assignee_id"], ["Teams", teams, "team_id"]].map(([title, rows, id]) => <section key={title as string} className="mt-6"><h2 className="mb-3 font-semibold">{title as string}</h2><table className="w-full text-left text-sm"><thead><tr><th>Name</th><th>Open</th><th>Unattended</th><th>Unassigned</th></tr></thead><tbody>{(rows as Metrics[]).map(row => <tr key={row[id as string]} className="border-t border-slate-200"><td className="py-3">{names[id as string]?.[row[id as string]] ?? row[id as string]}</td><td>{row.open}</td><td>{row.unattended}</td><td>{row.unassigned}</td></tr>)}</tbody></table></section>)}</div>;
}
