import {
  Activity,
  Building2,
  Layers,
  LayoutDashboard,
  LogOut,
  Settings,
  Shield,
  Users,
} from "lucide-react";
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
    return (
      <div className="flex min-h-screen items-center justify-center bg-n-background text-sm text-n-slate-10">
        Verifying SuperAdmin session...
      </div>
    );
  }
  if (forbidden) {
    return (
      <div className="flex min-h-screen flex-col items-center justify-center gap-3 bg-n-background p-6">
        <Shield size={28} className="text-n-ruby-11" />
        <h1 className="text-lg font-semibold text-n-slate-12">SuperAdmin privileges required</h1>
        <Link to="/super_admin/sign_in" className="text-sm text-n-brand hover:underline">
          Sign in as SuperAdmin
        </Link>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen bg-n-background text-n-slate-12">
      <aside className="flex w-60 shrink-0 flex-col border-r border-n-weak bg-n-surface-1 p-3">
        <div className="mb-5 flex items-center gap-2.5 px-2 pt-1">
          <img
            src="/app/brand-assets/logo_thumbnail.svg"
            alt="Pulsar"
            className="size-6 shrink-0"
          />
          <div className="min-w-0">
            <div className="text-sm font-semibold leading-tight text-n-slate-12">Super Admin</div>
            <div className="text-[11px] text-n-slate-10">Instance Console</div>
          </div>
        </div>
        <nav className="flex-1 space-y-1">
          {navItems.map((item) => {
            const Icon = item.icon;
            const active = item.exact ? pathname.endsWith("/super_admin") : pathname.includes(item.to);
            return (
              <Link
                key={item.to}
                to={item.to}
                className={`flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-sm transition-colors ${
                  active
                    ? "bg-n-alpha-2 font-medium text-n-blue-11 dark:text-n-slate-12"
                    : "text-n-slate-11 hover:bg-n-alpha-1 hover:text-n-slate-12"
                }`}
              >
                <Icon size={16} />
                {item.label}
              </Link>
            );
          })}
        </nav>
        <div className="space-y-1 border-t border-n-weak pt-3 text-sm">
          <Link
            to="/"
            className="block rounded-lg px-2.5 py-1.5 text-n-slate-11 hover:bg-n-alpha-1 hover:text-n-slate-12"
          >
            ← Back to App
          </Link>
          <button
            onClick={() => void signOut()}
            className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-n-ruby-11 hover:bg-n-ruby-3"
          >
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
