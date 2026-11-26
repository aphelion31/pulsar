import {
  Check,
  ChevronRight,
  Globe,
  Keyboard,
  LogOut,
  Monitor,
  Moon,
  Palette,
  Settings,
  Shield,
  Sun,
  User,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router";
import Avatar from "~/components/next/Avatar";
import { api, signOut, type Profile } from "~/lib/api";
import { getLocale, setLocale, SUPPORTED_LOCALES } from "~/lib/i18n";
import { getTheme, setTheme, type ThemeMode } from "~/lib/theme";
import { openCommandBar } from "~/components/next/CommandBar";

export type SidebarProfileMenuProps = {
  me: Profile;
  accountId: number;
  isCollapsed?: boolean;
  onOpenShortcuts?: () => void;
};

const AVAILABILITIES: Array<{ key: "online" | "busy" | "offline"; label: string; dot: string }> = [
  { key: "online", label: "Online", dot: "bg-n-teal-9" },
  { key: "busy", label: "Busy", dot: "bg-n-amber-9" },
  { key: "offline", label: "Offline", dot: "bg-n-slate-8" },
];

export default function SidebarProfileMenu({
  me,
  accountId,
  isCollapsed = false,
  onOpenShortcuts,
}: SidebarProfileMenuProps) {
  const [open, setOpen] = useState(false);
  const [availability, setAvailability] = useState<string>(
    () => (me.accounts.find((a) => a.id === accountId) ?? me.accounts[0])?.availability || "online"
  );
  const [theme, setThemeState] = useState<ThemeMode>(getTheme);
  const [locale, setLocaleState] = useState<string>(getLocale);
  const containerRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();

  useEffect(() => {
    const syncAvail = (e: Event) => {
      const next = (e as CustomEvent<string>).detail;
      if (next) setAvailability(next);
    };
    const syncTheme = () => setThemeState(getTheme());
    const syncLocale = () => setLocaleState(getLocale());
    window.addEventListener("pulsar:availability-changed", syncAvail);
    window.addEventListener("pulsar:theme-changed", syncTheme);
    window.addEventListener("pulsar:locale-changed", syncLocale);
    return () => {
      window.removeEventListener("pulsar:availability-changed", syncAvail);
      window.removeEventListener("pulsar:theme-changed", syncTheme);
      window.removeEventListener("pulsar:locale-changed", syncLocale);
    };
  }, []);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [open]);

  const changeAvailability = async (next: "online" | "busy" | "offline") => {
    setAvailability(next);
    await api("/api/v1/profile/availability", {
      method: "POST",
      json: { profile: { account_id: accountId, availability: next } },
    }).catch(() => {});
    window.dispatchEvent(new CustomEvent("pulsar:availability-changed", { detail: next }));
  };

  const handleLogout = async () => {
    setOpen(false);
    await signOut();
    navigate("/login", { replace: true });
  };

  const avatarStatus: "online" | "busy" | "offline" =
    availability === "busy" ? "busy" : availability === "offline" ? "offline" : "online";

  return (
    <div ref={containerRef} className="relative min-w-0 w-full">
      <button
        type="button"
        className={`flex items-center gap-2 p-1 text-left rounded-lg cursor-pointer hover:bg-n-alpha-1 transition-colors ${
          open ? "bg-n-alpha-1" : ""
        } ${isCollapsed ? "justify-center w-auto mx-auto" : "w-full"}`}
        title={isCollapsed ? me.available_name : undefined}
        onClick={() => setOpen(!open)}
      >
        <Avatar
          size={32}
          name={me.available_name}
          src={me.avatar_url || undefined}
          status={avatarStatus}
        />
        {!isCollapsed && (
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium leading-4 truncate text-n-slate-12">
              {me.available_name}
            </div>
            <div className="text-xs truncate text-n-slate-11">{me.email}</div>
          </div>
        )}
      </button>

      {open && (
        <div className="absolute bottom-full left-0 mb-2 z-50 w-72 rounded-xl bg-n-solid-1 shadow-xl border border-n-weak p-2">
          {/* Availability Switcher */}
          <div className="px-2 py-1.5 border-b border-n-weak mb-1.5">
            <div className="mb-1.5 text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
              Availability
            </div>
            <div className="grid grid-cols-3 gap-1">
              {AVAILABILITIES.map((a) => {
                const active = availability === a.key;
                return (
                  <button
                    key={a.key}
                    type="button"
                    onClick={() => void changeAvailability(a.key)}
                    className={`flex items-center justify-center gap-1.5 rounded-lg px-2 py-1 text-xs font-medium transition-colors ${
                      active
                        ? "bg-n-alpha-2 text-n-slate-12 ring-1 ring-n-weak"
                        : "text-n-slate-11 hover:bg-n-alpha-1 hover:text-n-slate-12"
                    }`}
                  >
                    <span className={`size-2 rounded-full ${a.dot}`} />
                    {a.label}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Theme quick toggle */}
          <div className="px-2 py-1.5 border-b border-n-weak mb-1">
            <div className="mb-1.5 flex items-center justify-between">
              <span className="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
                <Palette className="size-3.5" /> Appearance
              </span>
              <button
                type="button"
                onClick={() => {
                  setOpen(false);
                  openCommandBar({ page: "appearance" });
                }}
                className="flex items-center gap-0.5 text-[11px] text-n-blue-11 hover:underline"
              >
                ⌘K <ChevronRight className="size-3" />
              </button>
            </div>
            <div className="grid grid-cols-3 gap-1">
              {(
                [
                  { id: "light", label: "Light", icon: Sun },
                  { id: "dark", label: "Dark", icon: Moon },
                  { id: "auto", label: "System", icon: Monitor },
                ] as const
              ).map((item) => {
                const Icon = item.icon;
                const active = theme === item.id;
                return (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => {
                      setTheme(item.id);
                      setThemeState(item.id);
                    }}
                    className={`flex items-center justify-center gap-1 rounded-lg px-2 py-1 text-xs font-medium transition-colors ${
                      active
                        ? "bg-n-blue-3 text-n-blue-11 dark:bg-n-alpha-2 dark:text-n-slate-12"
                        : "text-n-slate-11 hover:bg-n-alpha-1 hover:text-n-slate-12"
                    }`}
                  >
                    <Icon className="size-3.5" />
                    {item.label}
                    {active && <Check className="size-3" />}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Language selector */}
          <div className="px-2 py-1.5 border-b border-n-weak mb-1">
            <label className="flex items-center justify-between gap-2 text-xs text-n-slate-11">
              <span className="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
                <Globe className="size-3.5" /> Language
              </span>
              <select
                aria-label="Language"
                value={locale}
                onChange={(e) => {
                  const next = e.target.value;
                  setLocaleState(next);
                  void setLocale(next);
                }}
                className="h-7 max-w-36 rounded-md border border-n-weak bg-n-background px-2 text-xs text-n-slate-12 outline-none focus:border-n-brand"
              >
                {SUPPORTED_LOCALES.map((l) => (
                  <option key={l.code} value={l.code}>
                    {l.name}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <ul className="m-0 list-none">
            <li>
              <button
                type="button"
                className="flex items-center gap-2.5 w-full px-2 py-1.5 rounded-lg text-sm text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12 transition-colors text-left"
                onClick={() => {
                  setOpen(false);
                  onOpenShortcuts?.();
                }}
              >
                <Keyboard className="size-4 shrink-0" />
                <span>Keyboard shortcuts</span>
              </button>
            </li>
            <li>
              <Link
                to={`/accounts/${accountId}/profile/settings`}
                className="flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-sm text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12 transition-colors"
                onClick={() => setOpen(false)}
              >
                <User className="size-4 shrink-0" />
                <span>Profile settings</span>
              </Link>
            </li>
            <li>
              <Link
                to={`/accounts/${accountId}/settings`}
                className="flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-sm text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12 transition-colors"
                onClick={() => setOpen(false)}
              >
                <Settings className="size-4 shrink-0" />
                <span>Account settings</span>
              </Link>
            </li>
            {me.type === "SuperAdmin" && (
              <li>
                <Link
                  to="/super_admin"
                  className="flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-sm text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12 transition-colors"
                  onClick={() => setOpen(false)}
                >
                  <Shield className="size-4 shrink-0" />
                  <span>Super Admin Console</span>
                </Link>
              </li>
            )}
            <li className="my-1 border-t border-n-weak" />
            <li>
              <button
                type="button"
                className="flex items-center gap-2.5 w-full px-2 py-1.5 rounded-lg text-sm text-n-ruby-11 hover:bg-n-ruby-3 transition-colors text-left font-medium"
                onClick={handleLogout}
              >
                <LogOut className="size-4 shrink-0" />
                <span>Log out</span>
              </button>
            </li>
          </ul>
        </div>
      )}
    </div>
  );
}
