// Command Bar (mirrors Chatwoot @chatwoot/ninja-keys CMD+K menu).
import {
  BarChart3,
  BookOpen,
  Bot,
  Building2,
  Check,
  Inbox,
  Keyboard,
  Megaphone,
  MessageSquare,
  Monitor,
  Moon,
  Palette,
  Phone,
  Search,
  Settings,
  Shield,
  Sun,
  Users,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { api, type Profile } from "~/lib/api";
import { getTheme, setTheme, type ThemeMode } from "~/lib/theme";

type CommandAction = {
  id: string;
  section: string;
  title: string;
  keywords?: string;
  icon: React.ComponentType<{ className?: string }>;
  badge?: string;
  run: () => void | Promise<void>;
};

export function openCommandBar(detail?: { page?: "root" | "appearance" }) {
  window.dispatchEvent(new CustomEvent("pulsar:open-command-bar", { detail }));
}

export function CommandBar({
  me,
  accountId,
  onOpenShortcuts,
}: {
  me: Profile;
  accountId: number;
  onOpenShortcuts: () => void;
}) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [page, setPage] = useState<"root" | "appearance">("root");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [theme, setThemeState] = useState<ThemeMode>(getTheme);
  const inputRef = useRef<HTMLInputElement>(null);

  const currentAccount = me.accounts.find((a) => a.id === accountId) ?? me.accounts[0];
  const currentAvail = currentAccount?.availability ?? "online";

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPage("root");
        setOpen((v) => !v);
      } else if (e.key === "Escape" && open) {
        setOpen(false);
      }
    };
    const onCustom = (e: Event) => {
      const detail = (e as CustomEvent<{ page?: "root" | "appearance" }>).detail;
      setPage(detail?.page ?? "root");
      setOpen(true);
    };
    const onTheme = () => setThemeState(getTheme());
    window.addEventListener("keydown", onKey);
    window.addEventListener("pulsar:open-command-bar", onCustom);
    window.addEventListener("pulsar:theme-changed", onTheme);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pulsar:open-command-bar", onCustom);
      window.removeEventListener("pulsar:theme-changed", onTheme);
    };
  }, [open]);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelected(0);
      setThemeState(getTheme());
      const t = setTimeout(() => inputRef.current?.focus(), 20);
      return () => clearTimeout(t);
    }
  }, [open, page]);

  const setAvailability = async (availability: "online" | "busy" | "offline") => {
    await api("/api/v1/profile/availability", {
      method: "POST",
      json: { profile: { account_id: accountId, availability } },
    }).catch(() => {});
    window.dispatchEvent(new CustomEvent("pulsar:availability-changed", { detail: availability }));
    setOpen(false);
  };

  const actions = useMemo<CommandAction[]>(() => {
    const p = `/accounts/${accountId}`;
    const go = (to: string) => () => {
      setOpen(false);
      navigate(to);
    };

    if (page === "appearance") {
      return [
        {
          id: "theme-light",
          section: "Appearance",
          title: "Light",
          icon: Sun,
          badge: theme === "light" ? "Active" : undefined,
          run: () => { setTheme("light"); setOpen(false); },
        },
        {
          id: "theme-dark",
          section: "Appearance",
          title: "Dark",
          icon: Moon,
          badge: theme === "dark" ? "Active" : undefined,
          run: () => { setTheme("dark"); setOpen(false); },
        },
        {
          id: "theme-auto",
          section: "Appearance",
          title: "System",
          icon: Monitor,
          badge: theme === "auto" ? "Active" : undefined,
          run: () => { setTheme("auto"); setOpen(false); },
        },
      ];
    }

    const base: CommandAction[] = [
      ...(query.trim()
        ? [
            {
              id: "full-search",
              section: "Search",
              title: `Search conversations, contacts & messages for "${query.trim()}"`,
              icon: Search,
              run: go(`${p}/search?q=${encodeURIComponent(query.trim())}`),
            },
          ]
        : []),
      { id: "nav-inbox-view", section: "Go to", title: "My Inbox", keywords: "notifications inbox view", icon: Inbox, run: go(`${p}/inbox-view`) },
      { id: "nav-conversations", section: "Go to", title: "All Conversations", keywords: "chats messages tickets", icon: MessageSquare, run: go(`${p}/conversations`) },
      { id: "nav-contacts", section: "Go to", title: "Contacts", keywords: "customers users", icon: Users, run: go(`${p}/contacts`) },
      { id: "nav-companies", section: "Go to", title: "Companies", keywords: "organizations", icon: Building2, run: go(`${p}/companies`) },
      { id: "nav-portals", section: "Go to", title: "Help Center", keywords: "kb docs articles", icon: BookOpen, run: go(`${p}/portals`) },
      { id: "nav-calls", section: "Go to", title: "Voice Calls", keywords: "twilio phone", icon: Phone, run: go(`${p}/calls`) },
      { id: "nav-captain", section: "Go to", title: "Captain AI Assistants", keywords: "ai bot copilot", icon: Bot, run: go(`${p}/captain/assistants`) },
      { id: "nav-reports-live", section: "Reports", title: "Reports Overview (Live)", keywords: "analytics overview", icon: BarChart3, run: go(`${p}/reports/overview`) },
      { id: "nav-reports-conv", section: "Reports", title: "Conversation Reports", icon: BarChart3, run: go(`${p}/reports/conversation`) },
      { id: "nav-reports-csat", section: "Reports", title: "CSAT Reports", icon: BarChart3, run: go(`${p}/reports/csat`) },
      { id: "nav-reports-sla", section: "Reports", title: "SLA Reports", icon: BarChart3, run: go(`${p}/reports/sla`) },
      { id: "nav-campaigns", section: "Go to", title: "Campaigns", icon: Megaphone, run: go(`${p}/campaigns`) },
      { id: "nav-settings-general", section: "Settings", title: "Account Settings", icon: Settings, run: go(`${p}/settings/general`) },
      { id: "nav-settings-agents", section: "Settings", title: "Agents", icon: Settings, run: go(`${p}/settings/agents/list`) },
      { id: "nav-settings-inboxes", section: "Settings", title: "Inboxes", icon: Settings, run: go(`${p}/settings/inboxes/list`) },
      { id: "nav-settings-macros", section: "Settings", title: "Macros", icon: Settings, run: go(`${p}/settings/macros`) },
      { id: "nav-settings-integrations", section: "Settings", title: "Integrations", icon: Settings, run: go(`${p}/settings/integrations`) },
      { id: "nav-settings-security", section: "Settings", title: "Security & SAML", icon: Settings, run: go(`${p}/settings/security`) },
      { id: "nav-settings-billing", section: "Settings", title: "Billing", icon: Settings, run: go(`${p}/settings/billing`) },
      ...(me.type === "SuperAdmin"
        ? [{ id: "nav-super-admin", section: "Go to", title: "Super Admin Console", icon: Shield, run: go("/super_admin") }]
        : []),
      { id: "avail-online", section: "Availability", title: "Set availability: Online", badge: currentAvail === "online" ? "Current" : undefined, icon: Check, run: () => setAvailability("online") },
      { id: "avail-busy", section: "Availability", title: "Set availability: Busy", badge: currentAvail === "busy" ? "Current" : undefined, icon: Check, run: () => setAvailability("busy") },
      { id: "avail-offline", section: "Availability", title: "Set availability: Offline", badge: currentAvail === "offline" ? "Current" : undefined, icon: Check, run: () => setAvailability("offline") },
      { id: "theme-page", section: "Preferences", title: "Change Appearance (Light / Dark / System)", keywords: "dark light theme mode", icon: Palette, run: () => setPage("appearance") },
      {
        id: "shortcuts",
        section: "Preferences",
        title: "Keyboard Shortcuts",
        icon: Keyboard,
        run: () => {
          setOpen(false);
          onOpenShortcuts();
        },
      },
    ];
    return base;
  }, [accountId, currentAvail, me.type, navigate, onOpenShortcuts, page, query, theme]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return actions;
    return actions.filter(
      (a) =>
        a.title.toLowerCase().includes(q) ||
        a.section.toLowerCase().includes(q) ||
        (a.keywords?.toLowerCase().includes(q) ?? false)
    );
  }, [actions, query]);

  useEffect(() => {
    setSelected(0);
  }, [filtered.length]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-start justify-center bg-n-overlay-default px-4 pt-[12vh] backdrop-blur-[1px]"
      onClick={() => setOpen(false)}
      role="dialog"
      aria-modal="true"
      aria-label="Command bar"
    >
      <div
        className="w-full max-w-xl overflow-hidden rounded-xl border border-n-weak bg-n-solid-1 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-n-weak px-3.5 py-2.5">
          <Search className="size-4 shrink-0 text-n-slate-10" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelected((i) => Math.min(filtered.length - 1, i + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelected((i) => Math.max(0, i - 1));
              } else if (e.key === "Enter" && filtered[selected]) {
                e.preventDefault();
                void filtered[selected].run();
              }
            }}
            placeholder={
              page === "appearance"
                ? "Select theme appearance..."
                : "Type a command or search..."
            }
            className="w-full bg-transparent text-sm text-n-slate-12 placeholder:text-n-slate-10 focus:outline-none"
          />
          {page !== "root" && (
            <button
              type="button"
              onClick={() => setPage("root")}
              className="rounded px-1.5 py-0.5 text-xs text-n-slate-11 hover:bg-n-alpha-2"
            >
              Back
            </button>
          )}
          <kbd className="rounded border border-n-weak px-1.5 py-0.5 font-mono text-[11px] text-n-slate-10">
            ESC
          </kbd>
        </div>

        <div className="max-h-[55vh] overflow-y-auto p-1.5">
          {filtered.length === 0 ? (
            <p className="px-3 py-8 text-center text-sm text-n-slate-10">
              No matching commands
            </p>
          ) : (
            filtered.map((item, i) => {
              const Icon = item.icon;
              const prevSection = i > 0 ? filtered[i - 1].section : null;
              const isSelected = i === selected;
              return (
                <div key={item.id}>
                  {item.section !== prevSection && (
                    <div className="px-2.5 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
                      {item.section}
                    </div>
                  )}
                  <button
                    type="button"
                    onMouseEnter={() => setSelected(i)}
                    onClick={() => void item.run()}
                    className={`flex w-full items-center justify-between rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${
                      isSelected
                        ? "bg-n-blue-3/60 text-n-blue-11 dark:bg-n-alpha-2 dark:text-n-slate-12"
                        : "text-n-slate-12 hover:bg-n-alpha-1"
                    }`}
                  >
                    <span className="flex items-center gap-2.5 min-w-0">
                      <Icon className="size-4 shrink-0 text-n-slate-11" />
                      <span className="truncate">{item.title}</span>
                    </span>
                    {item.badge && (
                      <span className="rounded bg-n-alpha-2 px-1.5 py-0.5 text-[11px] text-n-slate-11">
                        {item.badge}
                      </span>
                    )}
                  </button>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
