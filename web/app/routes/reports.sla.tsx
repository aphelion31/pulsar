import { useEffect, useState } from "react";
import { Download } from "lucide-react";
import { Link, useOutletContext } from "react-router";
import { Button, ErrorText, PageHeader } from "~/components/ui";
import { acct, api, getSession } from "~/lib/api";
import type { ShellCtx } from "./shell";

type SlaMetrics = {
  total_applied_slas: number;
  number_of_sla_misses: number;
  hit_rate: string;
};

type AppliedSlaRow = {
  applied_sla: {
    id: number;
    sla_name: string;
    sla_status: string;
    created_at: number;
  };
  conversation: {
    id: number;
    contact?: { name?: string | null } | null;
    labels?: string | null;
    assignee?: { id: number; name: string } | null;
  };
  sla_events: { id: number; event_type: string; created_at: number }[];
};

type FilterOption = { id: number; name?: string; title?: string };

export default function SlaReportsRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = acct(accountId);
  const [since, setSince] = useState(() => {
    const d = new Date(Date.now() - 29 * 86400000);
    return d.toISOString().slice(0, 10);
  });
  const [until, setUntil] = useState(() => new Date().toISOString().slice(0, 10));
  const [inboxId, setInboxId] = useState("");
  const [teamId, setTeamId] = useState("");
  const [agentId, setAgentId] = useState("");
  const [policyId, setPolicyId] = useState("");
  const [page, setPage] = useState(1);

  const [inboxes, setInboxes] = useState<FilterOption[]>([]);
  const [teams, setTeams] = useState<FilterOption[]>([]);
  const [agents, setAgents] = useState<FilterOption[]>([]);
  const [policies, setPolicies] = useState<FilterOption[]>([]);

  const [metrics, setMetrics] = useState<SlaMetrics>({
    total_applied_slas: 0,
    number_of_sla_misses: 0,
    hit_rate: "100%",
  });
  const [rows, setRows] = useState<AppliedSlaRow[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    let alive = true;
    Promise.all([
      api<{ payload: FilterOption[] } | FilterOption[]>(`${base}/inboxes`).catch(() => []),
      api<{ payload: FilterOption[] } | FilterOption[]>(`${base}/teams`).catch(() => []),
      api<FilterOption[]>(`${base}/agents`).catch(() => []),
      api<{ payload: FilterOption[] } | FilterOption[]>(`${base}/sla_policies`).catch(() => []),
    ]).then(([inb, tm, ag, pol]) => {
      if (!alive) return;
      setInboxes(Array.isArray(inb) ? inb : inb.payload ?? []);
      setTeams(Array.isArray(tm) ? tm : tm.payload ?? []);
      setAgents(Array.isArray(ag) ? ag : []);
      setPolicies(Array.isArray(pol) ? pol : pol.payload ?? []);
    });
    return () => {
      alive = false;
    };
  }, [base]);

  function queryParams(includePage: boolean) {
    const q = new URLSearchParams();
    if (since) q.set("since", String(Math.floor(new Date(`${since}T00:00:00Z`).getTime() / 1000)));
    if (until) q.set("until", String(Math.floor(new Date(`${until}T23:59:59Z`).getTime() / 1000)));
    if (inboxId) q.set("inbox_id", inboxId);
    if (teamId) q.set("team_id", teamId);
    if (agentId) q.set("assigned_agent_id", agentId);
    if (policyId) q.set("sla_policy_id", policyId);
    if (includePage) q.set("page", String(page));
    return q.toString();
  }

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError("");
    const qs = queryParams(true);
    const mqs = queryParams(false);
    Promise.all([
      api<SlaMetrics>(`${base}/applied_slas/metrics?${mqs}`),
      api<{ payload: AppliedSlaRow[]; meta: { count: number } }>(`${base}/applied_slas?${qs}`),
    ])
      .then(([m, list]) => {
        if (!alive) return;
        setMetrics(m);
        setRows(list.payload ?? []);
        setTotalCount(list.meta?.count ?? 0);
      })
      .catch((e) => {
        if (alive) setError(e instanceof Error ? e.message : "Failed to load SLA reports");
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [base, since, until, inboxId, teamId, agentId, policyId, page]);

  async function downloadCsv() {
    const s = getSession();
    const headers: Record<string, string> = {};
    if (s) {
      headers["access-token"] = s["access-token"];
      headers["client"] = s.client;
      headers["uid"] = s.uid;
    }
    const res = await fetch(`${base}/applied_slas/download?${queryParams(false)}`, { headers });
    if (!res.ok) {
      setError("Failed to download CSV");
      return;
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "breached_conversation.csv";
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title="SLA Reports">
        <Button variant="quiet" onClick={downloadCsv}>
          <Download className="size-4" />
          Download CSV
        </Button>
      </PageHeader>

      <div className="flex-1 space-y-6 overflow-y-auto p-6">
        <div className="grid gap-4 sm:grid-cols-3">
          <div className="rounded-lg border border-line bg-bg p-4">
            <p className="text-xs text-ink-3">Hit Rate</p>
            <p className="mt-1 text-2xl font-semibold text-ink">{metrics.hit_rate}</p>
          </div>
          <div className="rounded-lg border border-line bg-bg p-4">
            <p className="text-xs text-ink-3">Number of SLA Misses</p>
            <p className="mt-1 text-2xl font-semibold text-ink">{metrics.number_of_sla_misses}</p>
          </div>
          <div className="rounded-lg border border-line bg-bg p-4">
            <p className="text-xs text-ink-3">Total Applied SLAs</p>
            <p className="mt-1 text-2xl font-semibold text-ink">{metrics.total_applied_slas}</p>
          </div>
        </div>

        <div className="flex flex-wrap gap-3 rounded-lg border border-line p-3 text-sm">
          <input
            type="date"
            aria-label="Since"
            value={since}
            onChange={(e) => {
              setSince(e.target.value);
              setPage(1);
            }}
            className="rounded border border-line bg-surface px-2.5 py-1.5"
          />
          <input
            type="date"
            aria-label="Until"
            value={until}
            onChange={(e) => {
              setUntil(e.target.value);
              setPage(1);
            }}
            className="rounded border border-line bg-surface px-2.5 py-1.5"
          />
          <select
            aria-label="SLA policy"
            value={policyId}
            onChange={(e) => {
              setPolicyId(e.target.value);
              setPage(1);
            }}
            className="rounded border border-line bg-surface px-2.5 py-1.5"
          >
            <option value="">All policies</option>
            {policies.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <select
            aria-label="Inbox"
            value={inboxId}
            onChange={(e) => {
              setInboxId(e.target.value);
              setPage(1);
            }}
            className="rounded border border-line bg-surface px-2.5 py-1.5"
          >
            <option value="">All inboxes</option>
            {inboxes.map((i) => (
              <option key={i.id} value={i.id}>
                {i.name}
              </option>
            ))}
          </select>
          <select
            aria-label="Team"
            value={teamId}
            onChange={(e) => {
              setTeamId(e.target.value);
              setPage(1);
            }}
            className="rounded border border-line bg-surface px-2.5 py-1.5"
          >
            <option value="">All teams</option>
            {teams.map((tm) => (
              <option key={tm.id} value={tm.id}>
                {tm.name}
              </option>
            ))}
          </select>
          <select
            aria-label="Agent"
            value={agentId}
            onChange={(e) => {
              setAgentId(e.target.value);
              setPage(1);
            }}
            className="rounded border border-line bg-surface px-2.5 py-1.5"
          >
            <option value="">All agents</option>
            {agents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </div>

        <ErrorText>{error}</ErrorText>

        <div className="overflow-x-auto rounded-lg border border-line">
          <table className="w-full text-left text-sm">
            <thead className="bg-bg text-ink-3">
              <tr>
                <th className="px-4 py-3 font-medium">Conversation</th>
                <th className="px-4 py-3 font-medium">SLA Policy</th>
                <th className="px-4 py-3 font-medium">Breached Events</th>
                <th className="px-4 py-3 font-medium">Assignee</th>
                <th className="px-4 py-3 font-medium">Status</th>
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr>
                  <td colSpan={5} className="px-4 py-6 text-center text-ink-3">
                    Loading SLA data...
                  </td>
                </tr>
              ) : rows.length === 0 ? (
                <tr>
                  <td colSpan={5} className="px-4 py-6 text-center text-ink-3">
                    No breached SLAs in the selected range.
                  </td>
                </tr>
              ) : (
                rows.map((r) => (
                  <tr key={r.applied_sla.id} className="border-t border-line">
                    <td className="px-4 py-3">
                      <Link
                        className="font-medium text-accent underline"
                        to={`/accounts/${accountId}/conversations/${r.conversation.id}`}
                      >
                        #{r.conversation.id}
                      </Link>
                      {r.conversation.contact?.name && (
                        <span className="ml-2 text-ink-2">{r.conversation.contact.name}</span>
                      )}
                    </td>
                    <td className="px-4 py-3">{r.applied_sla.sla_name}</td>
                    <td className="px-4 py-3 uppercase">
                      {r.sla_events.map((ev) => ev.event_type).join(", ") || "-"}
                    </td>
                    <td className="px-4 py-3 text-ink-2">{r.conversation.assignee?.name ?? "Unassigned"}</td>
                    <td className="px-4 py-3">{r.applied_sla.sla_status}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>

        <div className="flex items-center justify-between text-sm">
          <Button variant="quiet" disabled={page <= 1} onClick={() => setPage((p) => p - 1)}>
            Previous
          </Button>
          <span className="text-ink-3">Page {page}</span>
          <Button variant="quiet" disabled={page * 25 >= totalCount} onClick={() => setPage((p) => p + 1)}>
            Next
          </Button>
        </div>
      </div>
    </section>
  );
}
