// Chatwoot LiveReports.vue + MetricCard.vue + AgentTable.vue parity.
import { useEffect, useState } from "react";
import Avatar from "~/components/next/Avatar";
import { api, type Agent } from "~/lib/api";

type Metrics = Record<string, number>;
type TeamItem = { id: number; name: string };

function LivePill() {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-md bg-n-teal-3 px-2 py-0.5 text-xs font-medium text-n-teal-11">
      <span className="size-1.5 rounded-full bg-n-teal-9 animate-pulse" />
      Live
    </span>
  );
}

export default function Live({ accountId }: { accountId: number }) {
  const [metrics, setMetrics] = useState<Metrics>({});
  const [agentMetrics, setAgentMetrics] = useState<Metrics[]>([]);
  const [teamMetrics, setTeamMetrics] = useState<Metrics[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [teams, setTeams] = useState<TeamItem[]>([]);
  const [selectedTeam, setSelectedTeam] = useState<string>("");
  const [error, setError] = useState("");

  useEffect(() => {
    const v1 = `/api/v1/accounts/${accountId}`;
    Promise.all([api<Agent[]>(`${v1}/agents`), api<TeamItem[]>(`${v1}/teams`)])
      .then(([a, t]) => {
        setAgents(Array.isArray(a) ? a : []);
        setTeams(Array.isArray(t) ? t : []);
      })
      .catch(() => {});
  }, [accountId]);

  useEffect(() => {
    let active = true;
    const base = `/api/v2/accounts/${accountId}/live_reports`;
    const teamQ = selectedTeam ? `?team_id=${selectedTeam}` : "";
    const load = () =>
      Promise.all([
        api<Metrics>(`${base}/conversation_metrics${teamQ}`),
        api<Metrics[]>(`${base}/grouped_conversation_metrics?group_by=assignee_id`),
        api<Metrics[]>(`${base}/grouped_conversation_metrics?group_by=team_id`),
      ])
        .then(([m, a, t]) => {
          if (!active) return;
          setMetrics(m || {});
          setAgentMetrics(Array.isArray(a) ? a : []);
          setTeamMetrics(Array.isArray(t) ? t : []);
          setError("");
        })
        .catch((err: unknown) => {
          if (active) setError(err instanceof Error ? err.message : "Failed to load live reports");
        });
    const poll = () => {
      if (typeof document !== "undefined" && document.hidden) return;
      void load();
    };
    void load();
    const timer = setInterval(poll, 60000);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [accountId, selectedTeam]);

  const statusCounts = {
    online: agents.filter((a) => a.availability_status === "online").length,
    busy: agents.filter((a) => a.availability_status === "busy").length,
    offline: agents.filter(
      (a) => a.availability_status !== "online" && a.availability_status !== "busy"
    ).length,
  };

  const agentMap = Object.fromEntries(agents.map((a) => [a.id, a]));
  const teamMap = Object.fromEntries(teams.map((t) => [t.id, t]));

  const metricCards = [
    { key: "open", label: "Open", value: metrics.open ?? 0 },
    { key: "unattended", label: "Unattended", value: metrics.unattended ?? 0 },
    { key: "unassigned", label: "Unassigned", value: metrics.unassigned ?? 0 },
    ...(metrics.pending !== undefined ? [{ key: "pending", label: "Pending", value: metrics.pending }] : []),
  ];

  return (
    <div className="h-full overflow-auto bg-n-background p-6">
      <div className="mx-auto max-w-6xl space-y-5">
        <header className="flex items-center justify-between">
          <h1 className="font-inter-display text-xl font-semibold text-n-slate-12">Overview</h1>
        </header>

        {error && (
          <p role="alert" className="rounded-lg bg-n-ruby-3 px-3 py-2 text-sm text-n-ruby-11">
            {error}
          </p>
        )}

        {/* 65% / 35% Chatwoot top split row */}
        <div className="flex flex-col gap-4 md:flex-row">
          <section className="flex flex-col rounded-xl bg-n-solid-2 px-6 py-5 shadow-xs outline outline-1 outline-n-weak md:w-[65%]">
            <div className="mb-6 flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2.5">
                <h2 className="text-base font-medium text-n-slate-12">Open Conversations</h2>
                <LivePill />
              </div>
              {teams.length > 0 && (
                <select
                  aria-label="Filter team"
                  value={selectedTeam}
                  onChange={(e) => setSelectedTeam(e.target.value)}
                  className="h-8 rounded-lg border border-n-weak bg-n-background px-2.5 text-xs font-medium text-n-slate-12 focus:border-n-brand focus:outline-none"
                >
                  <option value="">All Teams</option>
                  {teams.map((tm) => (
                    <option key={tm.id} value={String(tm.id)}>
                      {tm.name}
                    </option>
                  ))}
                </select>
              )}
            </div>
            <div className="grid grid-cols-3 gap-4 sm:grid-cols-4">
              {metricCards.map((item) => (
                <div key={item.key} className="min-w-0">
                  <h3 className="text-sm text-n-slate-11">{item.label}</h3>
                  <p className="mt-1.5 font-inter-display text-3xl font-semibold tabular-nums text-n-slate-12">
                    {item.value.toLocaleString()}
                  </p>
                </div>
              ))}
            </div>
          </section>

          <section className="flex flex-col rounded-xl bg-n-solid-2 px-6 py-5 shadow-xs outline outline-1 outline-n-weak md:w-[35%]">
            <div className="mb-6 flex items-center gap-2.5">
              <h2 className="text-base font-medium text-n-slate-12">Agent Status</h2>
              <LivePill />
            </div>
            <div className="grid grid-cols-3 gap-3">
              {[
                { label: "Online", val: statusCounts.online, dot: "bg-n-teal-9" },
                { label: "Busy", val: statusCounts.busy, dot: "bg-n-amber-9" },
                { label: "Offline", val: statusCounts.offline, dot: "bg-n-slate-8" },
              ].map((s) => (
                <div key={s.label}>
                  <div className="flex items-center gap-1.5 text-sm text-n-slate-11">
                    <span className={`size-2 rounded-full ${s.dot}`} />
                    {s.label}
                  </div>
                  <p className="mt-1.5 font-inter-display text-3xl font-semibold tabular-nums text-n-slate-12">
                    {s.val}
                  </p>
                </div>
              ))}
            </div>
          </section>
        </div>

        {/* Agent & Team MetricCards */}
        {[
          { title: "Agent Conversations", kind: "agent" as const, rows: agentMetrics },
          { title: "Team Conversations", kind: "team" as const, rows: teamMetrics },
        ].map((group) => (
          <section
            key={group.title}
            className="rounded-xl bg-n-solid-2 px-6 py-5 shadow-xs outline outline-1 outline-n-weak"
          >
            <div className="mb-4 flex items-center gap-2.5">
              <h2 className="text-base font-medium text-n-slate-12">{group.title}</h2>
              <LivePill />
            </div>
            {group.rows.length === 0 ? (
              <p className="py-6 text-center text-sm text-n-slate-10">
                No active {group.kind} metrics yet.
              </p>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-n-weak text-xs font-medium uppercase tracking-wider text-n-slate-10">
                      <th className="py-2.5 pr-4">{group.kind === "agent" ? "Agent" : "Team"}</th>
                      <th className="py-2.5 px-4 text-right">Open</th>
                      <th className="py-2.5 px-4 text-right">Unattended</th>
                      <th className="py-2.5 pl-4 text-right">Unassigned</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-n-weak">
                    {group.rows.map((row) => {
                      const rowId = row[group.kind === "agent" ? "assignee_id" : "team_id"];
                      const ag = group.kind === "agent" ? agentMap[rowId] : undefined;
                      const tm = group.kind === "team" ? teamMap[rowId] : undefined;
                      const label =
                        ag?.available_name ||
                        ag?.name ||
                        tm?.name ||
                        `${group.kind === "agent" ? "Agent" : "Team"} #${rowId}`;
                      return (
                        <tr key={rowId} className="hover:bg-n-alpha-1">
                          <td className="py-3 pr-4">
                            <div className="flex items-center gap-2.5">
                              {group.kind === "agent" && (
                                <Avatar
                                  size={28}
                                  name={label}
                                  src={ag?.thumbnail}
                                  status={
                                    ag?.availability_status === "online"
                                      ? "online"
                                      : ag?.availability_status === "busy"
                                        ? "busy"
                                        : "offline"
                                  }
                                />
                              )}
                              <span className="font-medium text-n-slate-12">{label}</span>
                            </div>
                          </td>
                          <td className="py-3 px-4 text-right font-mono tabular-nums text-n-slate-12">
                            {row.open ?? 0}
                          </td>
                          <td className="py-3 px-4 text-right font-mono tabular-nums text-n-slate-11">
                            {row.unattended ?? 0}
                          </td>
                          <td className="py-3 pl-4 text-right font-mono tabular-nums text-n-slate-11">
                            {row.unassigned ?? 0}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        ))}
      </div>
    </div>
  );
}
