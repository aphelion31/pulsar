import { Activity, Building2, Layers, LayoutDashboard, LogOut, Settings, Shield, Users } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, Outlet, useLocation, useNavigate } from "react-router";
import { ApiError, api } from "~/lib/api";

export default function SuperAdminLayout() {
  const nav = useNavigate();
  const { pathname } = useLocation();
  const [checked, setChecked] = useState(false);
  const [forbidden, setForbidden] = useState(false);

  useEffect(() => {
    let active = true;
    api("/super_admin/api/instance_status")
      .then(() => {
        if (active) setChecked(true);
      })
      .catch((err) => {
        if (!active) return;
        if (err instanceof ApiError && err.status === 401) {
          nav("/super_admin/sign_in", { replace: true });
        } else if (err instanceof ApiError && err.status === 403) {
          setForbidden(true);
          setChecked(true);
        } else {
          setChecked(true);
        }
      });
    return () => {
      active = false;
    };
  }, [nav]);

  async function signOut() {
    await fetch("/super_admin/logout");
    nav("/super_admin/sign_in", { replace: true });
  }

  const navItems = [
    { to: "/super_admin", label: "Dashboard", icon: LayoutDashboard, exact: true },
    { to: "/super_admin/accounts", label: "Accounts", icon: Building2 },
    { to: "/super_admin/users", label: "Users", icon: Users },
    { to: "/super_admin/configs", label: "App Configs", icon: Settings },
    { to: "/super_admin/resources", label: "Bots, Apps & Banners", icon: Layers },
    { to: "/super_admin/instance_status", label: "Instance Status", icon: Activity },
  ];

  if (!checked) {
    return <div className="flex min-h-screen items-center justify-center bg-bg text-sm text-ink-3">Verifying SuperAdmin session...</div>;
  }
  if (forbidden) {
    return (
      <div className="flex min-h-screen flex-col items-center justify-center gap-3 bg-bg p-6">
        <Shield size={28} className="text-bad" />
        <h1 className="text-lg font-semibold text-ink">SuperAdmin privileges required</h1>
        <Link to="/super_admin/sign_in" className="text-sm text-accent hover:underline">Sign in as SuperAdmin</Link>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen bg-bg text-ink">
      <aside className="flex w-60 shrink-0 flex-col border-r border-line bg-surface p-4">
        <div className="mb-6 flex items-center gap-2 px-2">
          <Shield size={18} className="text-accent" />
          <span className="font-semibold text-ink">Super Admin</span>
        </div>
        <nav className="flex-1 space-y-1">
          {navItems.map((item) => {
            const Icon = item.icon;
            const active = item.exact ? pathname.endsWith("/super_admin") : pathname.includes(item.to);
            return (
              <Link
                key={item.to}
                to={item.to}
                className={`flex items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors ${
                  active ? "bg-accent text-accent-ink font-medium" : "text-ink-2 hover:bg-raised hover:text-ink"
                }`}
              >
                <Icon size={15} />
                {item.label}
              </Link>
            );
          })}
        </nav>
        <div className="space-y-2 border-t border-line pt-3 text-sm">
          <Link to="/" className="block rounded-lg px-3 py-1.5 text-ink-2 hover:bg-raised">
            ← Back to App
          </Link>
          <button onClick={() => void signOut()} className="flex w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left text-bad hover:bg-bad/10">
            <LogOut size={14} /> Sign Out
          </button>
        </div>
      </aside>
      <main className="min-w-0 flex-1 overflow-y-auto p-6">
        <Outlet />
      </main>
    </div>
  );
}
