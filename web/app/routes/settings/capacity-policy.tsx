import { useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useOutletContext, useParams } from "react-router";
import { Button, ErrorText, Input, PageHeader } from "~/components/ui";
import { acct, api, type Agent, type Inbox } from "~/lib/api";
import type { ShellCtx } from "../shell";

type InboxLimit = { id: number; inbox_id: number; conversation_limit: number };
type CapacityPolicy = {
  id: number;
  name: string;
  description: string | null;
  exclusion_rules: { exclude_older_than_hours?: number | null; excluded_labels?: string[] };
  assigned_agent_count: number;
  inbox_capacity_limits: InboxLimit[];
};

export default function CapacityPolicyRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { id } = useParams();
  const path = useLocation().pathname;
  const navigate = useNavigate();
  const root = `/accounts/${accountId}/settings/assignment-policy/capacity`;
  const base = `${acct(accountId)}/agent_capacity_policies`;
  const isCreate = path.endsWith("/create");
  const isEdit = Boolean(id);

  const [policies, setPolicies] = useState<CapacityPolicy[]>([]);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [excludeHours, setExcludeHours] = useState("");
  const [excludedLabels, setExcludedLabels] = useState("");
  const [limits, setLimits] = useState<InboxLimit[]>([]);
  const [assignedAgents, setAssignedAgents] = useState<Agent[]>([]);
  const [allInboxes, setAllInboxes] = useState<Inbox[]>([]);
  const [allAgents, setAllAgents] = useState<Agent[]>([]);
  const [newInboxId, setNewInboxId] = useState("");
  const [newLimit, setNewLimit] = useState("5");
  const [newAgentId, setNewAgentId] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function loadList() {
    const list = await api<CapacityPolicy[]>(base);
    setPolicies(list);
  }

  async function loadEdit(policyId: string) {
    const [p, users, inboxes, agents] = await Promise.all([
      api<CapacityPolicy>(`${base}/${policyId}`),
      api<Agent[]>(`${base}/${policyId}/users`),
      api<{ payload: Inbox[] } | Inbox[]>(`${acct(accountId)}/inboxes`),
      api<Agent[]>(`${acct(accountId)}/agents`),
    ]);
    setName(p.name);
    setDescription(p.description ?? "");
    setExcludeHours(
      p.exclusion_rules?.exclude_older_than_hours != null
        ? String(p.exclusion_rules.exclude_older_than_hours)
        : "",
    );
    setExcludedLabels((p.exclusion_rules?.excluded_labels ?? []).join(", "));
    setLimits(p.inbox_capacity_limits ?? []);
    setAssignedAgents(users);
    setAllInboxes(Array.isArray(inboxes) ? inboxes : inboxes.payload ?? []);
    setAllAgents(agents);
  }

  useEffect(() => {
    let alive = true;
    setError("");
    if (isEdit && id) {
      loadEdit(id).catch((e) => {
        if (alive) setError(e instanceof Error ? e.message : "Failed to load policy");
      });
    } else if (!isCreate) {
      loadList().catch((e) => {
        if (alive) setError(e instanceof Error ? e.message : "Failed to load policies");
      });
    } else {
      setName("");
      setDescription("");
      setExcludeHours("");
      setExcludedLabels("");
      setLimits([]);
      setAssignedAgents([]);
    }
    return () => {
      alive = false;
    };
  }, [accountId, base, id, isCreate, isEdit]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const payload = {
        agent_capacity_policy: {
          name: name.trim(),
          description: description.trim() || null,
          exclusion_rules: {
            exclude_older_than_hours: excludeHours.trim() ? Number(excludeHours) : null,
            excluded_labels: excludedLabels
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean),
          },
        },
      };
      const saved = await api<CapacityPolicy>(isEdit && id ? `${base}/${id}` : base, {
        method: isEdit ? "PATCH" : "POST",
        json: payload,
      });
      navigate(`${root}/edit/${saved.id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save policy");
    } finally {
      setBusy(false);
    }
  }

  async function removePolicy(policyId: number) {
    if (!window.confirm("Delete this capacity policy?")) return;
    setError("");
    try {
      await api(`${base}/${policyId}`, { method: "DELETE" });
      await loadList();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete failed");
    }
  }

  async function addLimit(e: React.FormEvent) {
    e.preventDefault();
    if (!id || !newInboxId) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/${id}/inbox_limits`, {
        method: "POST",
        json: { inbox_id: Number(newInboxId), conversation_limit: Number(newLimit) },
      });
      setNewInboxId("");
      await loadEdit(id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add inbox limit");
    } finally {
      setBusy(false);
    }
  }

  async function deleteLimit(limitId: number) {
    if (!id) return;
    setBusy(true);
    try {
      await api(`${base}/${id}/inbox_limits/${limitId}`, { method: "DELETE" });
      await loadEdit(id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete inbox limit");
    } finally {
      setBusy(false);
    }
  }

  async function addAgent(e: React.FormEvent) {
    e.preventDefault();
    if (!id || !newAgentId) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/${id}/users`, {
        method: "POST",
        json: { user_id: Number(newAgentId) },
      });
      setNewAgentId("");
      await loadEdit(id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to assign agent");
    } finally {
      setBusy(false);
    }
  }

  async function removeAgent(userId: number) {
    if (!id) return;
    setBusy(true);
    try {
      await api(`${base}/${id}/users/${userId}`, { method: "DELETE" });
      await loadEdit(id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to unassign agent");
    } finally {
      setBusy(false);
    }
  }

  if (!isCreate && !isEdit) {
    return (
      <section className="flex min-w-0 flex-1 flex-col">
        <PageHeader title="Agent Capacity Policies">
          <Link to={`${root}/create`}>
            <Button>Create Capacity Policy</Button>
          </Link>
        </PageHeader>
        <div className="flex-1 space-y-4 overflow-y-auto p-6">
          <ErrorText>{error}</ErrorText>
          {policies.length === 0 ? (
            <p className="text-sm text-ink-3">No capacity policies created yet.</p>
          ) : (
            policies.map((p) => (
              <article key={p.id} className="flex items-start justify-between gap-4 rounded-lg border border-line p-4">
                <div>
                  <h2 className="font-medium text-ink">{p.name}</h2>
                  {p.description && <p className="mt-0.5 text-sm text-ink-3">{p.description}</p>}
                  <p className="mt-2 text-xs text-ink-2">
                    Assigned agents: {p.assigned_agent_count} · Inbox limits: {p.inbox_capacity_limits.length}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <Link className="text-sm text-accent underline" to={`${root}/edit/${p.id}`}>
                    Edit
                  </Link>
                  <Button variant="danger" onClick={() => removePolicy(p.id)}>
                    Delete
                  </Button>
                </div>
              </article>
            ))
          )}
        </div>
      </section>
    );
  }

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={isEdit ? "Edit Capacity Policy" : "New Capacity Policy"}>
        <Link to={root}>
          <Button variant="quiet">Back to policies</Button>
        </Link>
      </PageHeader>
      <div className="flex-1 space-y-8 overflow-y-auto p-6">
        <form onSubmit={save} className="max-w-2xl space-y-4">
          <ErrorText>{error}</ErrorText>
          <Input label="Policy name" value={name} onChange={(e) => setName(e.target.value)} required />
          <label className="grid gap-1.5 text-[13px] text-ink-2">
            Description
            <textarea
              rows={2}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="w-full rounded-lg border border-line-strong bg-surface p-3 text-sm text-ink outline-none focus:border-accent"
            />
          </label>
          <div className="grid gap-4 sm:grid-cols-2">
            <Input
              label="Exclude conversations older than (hours)"
              type="number"
              min={0}
              value={excludeHours}
              onChange={(e) => setExcludeHours(e.target.value)}
            />
            <Input
              label="Excluded labels (comma-separated)"
              value={excludedLabels}
              onChange={(e) => setExcludedLabels(e.target.value)}
              placeholder="vip, escalated"
            />
          </div>
          <Button type="submit" disabled={busy}>
            {isEdit ? "Save policy" : "Create policy"}
          </Button>
        </form>

        {isEdit && id && (
          <div className="grid max-w-2xl gap-8">
            <div className="space-y-3 rounded-lg border border-line p-4">
              <h2 className="text-sm font-medium">Inbox Conversation Limits</h2>
              {limits.map((l) => {
                const inboxName = allInboxes.find((i) => i.id === l.inbox_id)?.name ?? `Inbox #${l.inbox_id}`;
                return (
                  <div key={l.id} className="flex items-center justify-between border-b border-line py-2 text-sm">
                    <span>
                      {inboxName} — max <strong>{l.conversation_limit}</strong> conversations
                    </span>
                    <Button variant="danger" disabled={busy} onClick={() => deleteLimit(l.id)}>
                      Remove
                    </Button>
                  </div>
                );
              })}
              <form onSubmit={addLimit} className="flex flex-wrap items-end gap-3 pt-2">
                <label className="grid gap-1 text-xs text-ink-2">
                  Inbox
                  <select
                    value={newInboxId}
                    onChange={(e) => setNewInboxId(e.target.value)}
                    className="h-9 rounded-lg border border-line-strong bg-surface px-3 text-sm"
                  >
                    <option value="">Select inbox</option>
                    {allInboxes
                      .filter((i) => !limits.some((l) => l.inbox_id === i.id))
                      .map((i) => (
                        <option key={i.id} value={i.id}>
                          {i.name}
                        </option>
                      ))}
                  </select>
                </label>
                <label className="grid gap-1 text-xs text-ink-2">
                  Limit
                  <input
                    type="number"
                    min={0}
                    value={newLimit}
                    onChange={(e) => setNewLimit(e.target.value)}
                    className="h-9 w-24 rounded-lg border border-line-strong bg-surface px-3 text-sm"
                  />
                </label>
                <Button type="submit" disabled={busy || !newInboxId}>
                  Add inbox limit
                </Button>
              </form>
            </div>

            <div className="space-y-3 rounded-lg border border-line p-4">
              <h2 className="text-sm font-medium">Assigned Agents</h2>
              {assignedAgents.map((a) => (
                <div key={a.id} className="flex items-center justify-between border-b border-line py-2 text-sm">
                  <span>
                    {a.name} ({a.email})
                  </span>
                  <Button variant="quiet" disabled={busy} onClick={() => removeAgent(a.id)}>
                    Unassign
                  </Button>
                </div>
              ))}
              <form onSubmit={addAgent} className="flex flex-wrap items-end gap-3 pt-2">
                <label className="grid gap-1 text-xs text-ink-2">
                  Agent
                  <select
                    value={newAgentId}
                    onChange={(e) => setNewAgentId(e.target.value)}
                    className="h-9 rounded-lg border border-line-strong bg-surface px-3 text-sm"
                  >
                    <option value="">Select agent</option>
                    {allAgents
                      .filter((a) => !assignedAgents.some((x) => x.id === a.id))
                      .map((a) => (
                        <option key={a.id} value={a.id}>
                          {a.name} ({a.email})
                        </option>
                      ))}
                  </select>
                </label>
                <Button type="submit" disabled={busy || !newAgentId}>
                  Assign agent
                </Button>
              </form>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
