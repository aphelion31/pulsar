import { Building2, Inbox, MessagesSquare, Users } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router";
import { BarChart } from "~/components/next/charts/BarChart";
import { api } from "~/lib/api";

type DashboardData = {
  accountsCount: number;
  usersCount: number;
  inboxesCount: number;
  conversationsCount: number;
  messagesCount: number;
  chartData: [string, number][];
};

function formatAxisDay(isoDate: string): string {
  const parts = isoDate.split("-").map(Number);
  if (parts.length !== 3) return isoDate.slice(5);
  const d = new Date(parts[0], parts[1] - 1, parts[2]);
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

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

  // Pad last 30 days so sparse dev installations render the full Chatwoot 30-day timeline.
  const paddedSeries = useMemo(() => {
    const map = new Map<string, number>(data?.chartData ?? []);
    const days: [string, number][] = [];
    const today = new Date();
    for (let i = 29; i >= 0; i--) {
      const d = new Date(today.getFullYear(), today.getMonth(), today.getDate() - i);
      const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      days.push([key, map.get(key) ?? 0]);
    }
    return days;
  }, [data]);

  if (error) return <p className="text-sm text-n-ruby-11">{error}</p>;

  const metrics = [
    {
      label: "Accounts",
      val: data?.accountsCount,
      icon: Building2,
      tone: "bg-n-blue-3 text-n-blue-11",
      to: "/super_admin/accounts",
    },
    {
      label: "Users",
      val: data?.usersCount,
      icon: Users,
      tone: "bg-n-iris-3 text-n-iris-11",
      to: "/super_admin/users",
    },
    {
      label: "Inboxes",
      val: data?.inboxesCount,
      icon: Inbox,
      tone: "bg-n-teal-3 text-n-teal-11",
    },
    {
      label: "Conversations",
      val: data?.conversationsCount,
      icon: MessagesSquare,
      tone: "bg-n-amber-3 text-n-amber-11",
    },
  ];

  const total30 = paddedSeries.reduce((sum, [, c]) => sum + c, 0);
  const busiest = paddedSeries.reduce((best, cur) => (cur[1] > best[1] ? cur : best), paddedSeries[0]);

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="font-inter-display text-xl font-semibold text-n-slate-12">
            Admin Dashboard
          </h1>
          <p className="text-xs text-n-slate-11">
            Instance overview and 30-day conversation activity
          </p>
        </div>
      </div>

      {/* Chatwoot unified 4-card strip */}
      <ul className="m-0 grid list-none grid-cols-2 gap-px overflow-hidden rounded-xl border border-n-weak bg-n-weak lg:grid-cols-4">
        {metrics.map((item) => {
          const Icon = item.icon;
          const inner = (
            <div className={`flex flex-1 flex-col gap-3 p-4 ${item.to ? "hover:bg-n-solid-3 transition-colors" : ""}`}>
              <span className="flex items-center gap-2.5 text-sm text-n-slate-11">
                <span className={`grid size-8 shrink-0 place-items-center rounded-lg ${item.tone}`}>
                  <Icon className="size-4" />
                </span>
                {item.label}
              </span>
              <span className="font-inter-display text-3xl font-semibold tracking-tight tabular-nums text-n-slate-12">
                {data ? (item.val ?? 0).toLocaleString() : "—"}
              </span>
            </div>
          );
          return (
            <li key={item.label} className="flex bg-n-solid-1">
              {item.to ? (
                <Link to={item.to} className="flex flex-1">
                  {inner}
                </Link>
              ) : (
                inner
              )}
            </li>
          );
        })}
      </ul>

      {/* 30-day BarChart section matching superadmin_pages/views/dashboard/Index.vue */}
      <section className="rounded-xl border border-n-weak bg-n-solid-1 p-5">
        <div className="mb-4 flex flex-wrap items-start justify-between gap-x-8 gap-y-3">
          <div>
            <h2 className="text-base font-medium text-n-slate-12">Conversations</h2>
            <p className="text-xs text-n-slate-11">Created per day over the last 30 days</p>
          </div>
          {data && (
            <dl className="m-0 flex flex-wrap gap-x-8 gap-y-2">
              <div>
                <dt className="text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
                  Total
                </dt>
                <dd className="font-inter-display text-base font-semibold tabular-nums text-n-slate-12">
                  {total30.toLocaleString()}
                </dd>
              </div>
              <div>
                <dt className="text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
                  Daily average
                </dt>
                <dd className="font-inter-display text-base font-semibold tabular-nums text-n-slate-12">
                  {Math.round(total30 / 30).toLocaleString()}
                </dd>
              </div>
              <div>
                <dt className="text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
                  Busiest day
                </dt>
                <dd className="font-inter-display text-base font-semibold tabular-nums text-n-slate-12">
                  {busiest ? formatAxisDay(busiest[0]) : "—"}
                </dd>
              </div>
            </dl>
          )}
        </div>

        <BarChart
          height={300}
          ariaLabel="Conversations created by day"
          data={{
            categories: paddedSeries.map(([day]) => formatAxisDay(day)),
            series: [
              {
                id: "conversations",
                label: "Conversations",
                color: "#2781F6",
                data: paddedSeries.map(([, val]) => val),
              },
            ],
          }}
        />
      </section>
    </div>
  );
}
