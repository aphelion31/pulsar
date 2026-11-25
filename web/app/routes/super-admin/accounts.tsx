import { Plus, RefreshCw, Sparkles, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "~/components/ui";
import { api } from "~/lib/api";

type AccountRow = {
  id: number;
  name: string;
  domain: string | null;
  support_email: string | null;
  users_count: number;
  inboxes_count: number;
  conversations_count: number;
};

export default function SuperAdminAccountsRoute() {
  const [accounts, setAccounts] = useState<AccountRow[]>([]);
  const [name, setName] = useState("");
  const [domain, setDomain] = useState("");
  const [search, setSearch] = useState("");
  const [error, setError] = useState("");

  async function load() {
    setError("");
    try {
      const q = search.trim() ? `?q=${encodeURIComponent(search.trim())}` : "";
      const res = await api<{ payload: AccountRow[] }>(`/super_admin/api/accounts${q}`);
      setAccounts(res.payload ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load accounts");
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function createAccount(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    await api("/super_admin/api/accounts", {
      method: "POST",
      json: { account: { name: name.trim(), domain: domain.trim() || null } },
    });
    setName("");
    setDomain("");
    await load();
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold text-ink">Accounts</h1>
        <div className="flex gap-2">
          <input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search accounts..." className="h-9 rounded-lg border border-line bg-surface px-3 text-sm" />
          <Button variant="quiet" onClick={() => void load()}>Search</Button>
        </div>
      </div>

      <form onSubmit={createAccount} className="grid gap-3 rounded-xl border border-line bg-surface p-4 md:grid-cols-3">
        <input required value={name} onChange={(e) => setName(e.target.value)} placeholder="Account name" className="h-9 rounded-lg border border-line bg-bg px-3 text-sm" />
        <input value={domain} onChange={(e) => setDomain(e.target.value)} placeholder="Domain (optional)" className="h-9 rounded-lg border border-line bg-bg px-3 text-sm" />
        <Button type="submit"><Plus size={14} /> Create Account</Button>
      </form>

      {error && <p className="text-sm text-bad">{error}</p>}

      <div className="overflow-x-auto rounded-xl border border-line bg-surface">
        <table className="w-full text-left text-sm">
          <thead className="border-b border-line bg-raised text-xs uppercase text-ink-3">
            <tr>
              <th className="px-4 py-3">ID / Name</th>
              <th className="px-4 py-3">Users</th>
              <th className="px-4 py-3">Inboxes</th>
              <th className="px-4 py-3">Conversations</th>
              <th className="px-4 py-3 text-right">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {accounts.map((a) => (
              <tr key={a.id}>
                <td className="px-4 py-3">
                  <div className="font-medium text-ink">#{a.id} · {a.name}</div>
                  {a.domain && <div className="text-xs text-ink-3">{a.domain}</div>}
                </td>
                <td className="px-4 py-3 font-mono text-xs">{a.users_count}</td>
                <td className="px-4 py-3 font-mono text-xs">{a.inboxes_count}</td>
                <td className="px-4 py-3 font-mono text-xs">{a.conversations_count}</td>
                <td className="px-4 py-3 text-right space-x-3">
                  <button
                    onClick={async () => {
                      await api(`/super_admin/api/accounts/${a.id}/seed`, { method: "POST" });
                      await load();
                    }}
                    className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
                  >
                    <Sparkles size={12} /> Seed
                  </button>
                  <button
                    onClick={async () => {
                      await api(`/super_admin/api/accounts/${a.id}/reset_cache`, { method: "POST" });
                    }}
                    className="inline-flex items-center gap-1 text-xs text-ink-2 hover:underline"
                  >
                    <RefreshCw size={12} /> Reset Cache
                  </button>
                  <button
                    onClick={async () => {
                      await api(`/super_admin/api/accounts/${a.id}`, { method: "DELETE" });
                      await load();
                    }}
                    className="inline-flex items-center gap-1 text-xs text-bad hover:underline"
                  >
                    <Trash2 size={12} /> Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
