import { Plus, Trash2, UserCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { Button } from "~/components/ui";
import { api } from "~/lib/api";

type UserRow = {
  id: number;
  name: string;
  email: string | null;
  type: string | null;
  confirmed: boolean;
  sign_in_count: number;
};

export default function SuperAdminUsersRoute() {
  const nav = useNavigate();
  const [users, setUsers] = useState<UserRow[]>([]);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [isSuper, setIsSuper] = useState(false);
  const [error, setError] = useState("");

  async function load() {
    setError("");
    try {
      const res = await api<{ payload: UserRow[] }>("/super_admin/api/users");
      setUsers(res.payload ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load users");
    }
  }

  useEffect(() => {
    void load();
  }, []);

  async function createUser(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    try {
      await api("/super_admin/api/users", {
        method: "POST",
        json: { user: { name: name.trim(), email: email.trim(), password, type: isSuper ? "SuperAdmin" : null } },
      });
      setName("");
      setEmail("");
      setPassword("");
      setIsSuper(false);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create user");
    }
  }

  async function impersonate(id: number) {
    const res = await api<{ session: Record<string, string>; user: { account_id?: number } }>(
      `/super_admin/api/users/${id}/impersonate`,
      { method: "POST" },
    );
    localStorage.setItem("pulsar.session", JSON.stringify(res.session));
    nav(`/accounts/${res.user.account_id ?? 1}/conversations`);
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <h1 className="text-xl font-semibold text-ink">Users</h1>
      <form onSubmit={createUser} className="grid gap-3 rounded-xl border border-line bg-surface p-4 md:grid-cols-5">
        <input required value={name} onChange={(e) => setName(e.target.value)} placeholder="Name" className="h-9 rounded-lg border border-line bg-bg px-3 text-sm" />
        <input required type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="Email" className="h-9 rounded-lg border border-line bg-bg px-3 text-sm" />
        <input required type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="Password" className="h-9 rounded-lg border border-line bg-bg px-3 text-sm" />
        <label className="flex items-center gap-2 text-sm text-ink">
          <input type="checkbox" checked={isSuper} onChange={(e) => setIsSuper(e.target.checked)} /> SuperAdmin
        </label>
        <Button type="submit"><Plus size={14} /> Create User</Button>
      </form>
      {error && <p className="text-sm text-bad">{error}</p>}
      <div className="overflow-x-auto rounded-xl border border-line bg-surface">
        <table className="w-full text-left text-sm">
          <thead className="border-b border-line bg-raised text-xs uppercase text-ink-3">
            <tr>
              <th className="px-4 py-3">User</th>
              <th className="px-4 py-3">Type</th>
              <th className="px-4 py-3">Sign-ins</th>
              <th className="px-4 py-3 text-right">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {users.map((u) => (
              <tr key={u.id}>
                <td className="px-4 py-3">
                  <div className="font-medium text-ink">{u.name}</div>
                  <div className="text-xs text-ink-3">{u.email}</div>
                </td>
                <td className="px-4 py-3 text-xs text-ink-2">{u.type || "User"}</td>
                <td className="px-4 py-3 font-mono text-xs">{u.sign_in_count}</td>
                <td className="px-4 py-3 text-right space-x-3">
                  <button onClick={() => void impersonate(u.id)} className="inline-flex items-center gap-1 text-xs text-accent hover:underline">
                    <UserCheck size={12} /> Impersonate
                  </button>
                  <button
                    onClick={async () => {
                      await api(`/super_admin/api/users/${u.id}`, { method: "DELETE" });
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
