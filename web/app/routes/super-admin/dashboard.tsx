import { useEffect, useState } from "react";
import { api } from "~/lib/api";

type DashboardData = {
  accountsCount: number;
  usersCount: number;
  inboxesCount: number;
  conversationsCount: number;
  messagesCount: number;
  chartData: [string, number][];
};

export default function SuperAdminDashboardRoute() {
  const [data, setData] = useState<DashboardData | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    api<DashboardData>("/super_admin/api/dashboard")
      .then((d) => {
        if (active) setData(d);
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load dashboard");
      });
    return () => {
      active = false;
    };
  }, []);

  if (error) return <p className="text-sm text-bad">{error}</p>;
  if (!data) return <p className="text-sm text-ink-3">Loading dashboard...</p>;

  const maxBar = Math.max(1, ...data.chartData.map(([, c]) => c));

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <h1 className="text-xl font-semibold text-ink">Instance Overview</h1>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-5">
        {[
          { label: "Accounts", val: data.accountsCount },
          { label: "Users", val: data.usersCount },
          { label: "Inboxes", val: data.inboxesCount },
          { label: "Conversations", val: data.conversationsCount },
          { label: "Messages", val: data.messagesCount },
        ].map((card) => (
          <div key={card.label} className="rounded-xl border border-line bg-surface p-4">
            <div className="text-xs text-ink-3">{card.label}</div>
            <div className="mt-1 font-mono text-2xl font-semibold text-ink">{card.val}</div>
          </div>
        ))}
      </div>

      <div className="rounded-xl border border-line bg-surface p-5">
        <h2 className="mb-3 text-sm font-medium text-ink">Conversations (Last 30 Days)</h2>
        {data.chartData.length === 0 ? (
          <p className="text-sm text-ink-3">No recent conversations.</p>
        ) : (
          <div className="flex h-40 items-end gap-2">
            {data.chartData.map(([day, count]) => (
              <div key={day} className="flex flex-1 flex-col items-center gap-1">
                <div
                  className="w-full rounded-t bg-accent"
                  style={{ height: `${Math.max(8, (count / maxBar) * 100)}%` }}
                  title={`${day}: ${count}`}
                />
                <span className="text-[10px] text-ink-3">{day.slice(5)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
