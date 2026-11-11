import {
  Code,
  Globe,
  Mail,
  MessageSquare,
  PenLine,
  Phone,
  Search,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useLocation } from "react-router";
import { acct, api, type Inbox, type Label, type Profile, type Team } from "~/lib/api";
import SidebarAccountSwitcher, { Logo } from "./SidebarAccountSwitcher";
import SidebarGroup from "./SidebarGroup";
import SidebarProfileMenu from "./SidebarProfileMenu";

export type SidebarProps = {
  me: Profile;
  accountId: number;
  online?: boolean;
};

const DEFAULT_WIDTH = 240;
const MIN_WIDTH = 56;
const MAX_WIDTH = 320;
const COLLAPSED_THRESHOLD = 160;

function channelIcon(type?: string) {
  const t = (type || "").toLowerCase();
  if (t.includes("web") || t.includes("widget")) return <Globe className="size-4 shrink-0" />;
  if (t.includes("api")) return <Code className="size-4 shrink-0" />;
  if (t.includes("email") || t.includes("mail")) return <Mail className="size-4 shrink-0" />;
  if (t.includes("phone") || t.includes("whatsapp") || t.includes("sms"))
    return <Phone className="size-4 shrink-0" />;
  return <MessageSquare className="size-4 shrink-0" />;
}

export default function Sidebar({ me, accountId }: SidebarProps) {
  const location = useLocation();
  const currentAccount =
    me.accounts.find((a) => a.id === accountId) ?? me.accounts[0];

  const [inboxes, setInboxes] = useState<Inbox[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [teams, setTeams] = useState<Team[]>([]);

  // Width & collapse state
  const [sidebarWidth, setSidebarWidth] = useState(DEFAULT_WIDTH);
  const [isResizing, setIsResizing] = useState(false);
  const isCollapsed = sidebarWidth < COLLAPSED_THRESHOLD;

  // Active expanded group: defaults to "Conversation"
  const [expandedGroup, setExpandedGroup] = useState<string | null>("Conversation");

  // Fetch inboxes, labels, teams
  useEffect(() => {
    let active = true;
    const base = acct(accountId);

    api<Inbox[] | { payload: Inbox[] }>(`${base}/inboxes`)
      .then((r) => {
        if (!active) return;
        setInboxes(Array.isArray(r) ? r : r.payload || []);
      })
      .catch(() => {});

    api<Label[] | { payload: Label[] }>(`${base}/labels`)
      .then((r) => {
        if (!active) return;
        setLabels(Array.isArray(r) ? r : r.payload || []);
      })
      .catch(() => {});

    api<Team[] | { payload: Team[] }>(`${base}/teams`)
      .then((r) => {
        if (!active) return;
        setTeams(Array.isArray(r) ? r : r.payload || []);
      })
      .catch(() => {});

    return () => {
      active = false;
    };
  }, [accountId]);

  // Resize handling
  const startResize = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsResizing(true);
      const startX = e.clientX;
      const startW = sidebarWidth;

      const onMouseMove = (moveEvent: MouseEvent) => {
        const delta = moveEvent.clientX - startX;
        const newW = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, startW + delta));
        setSidebarWidth(newW);
      };

      const onMouseUp = () => {
        setIsResizing(false);
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    [sidebarWidth]
  );

  const toggleCollapsed = () => {
    setSidebarWidth((w) => (w < COLLAPSED_THRESHOLD ? DEFAULT_WIDTH : MIN_WIDTH));
  };

  const menuItems = useMemo(() => {
    const acctPrefix = `/accounts/${accountId}`;

    return [
      // 1. My Inbox
      {
        name: "Inbox",
        label: "My Inbox",
        icon: "i-lucide-inbox",
        to: `${acctPrefix}/inbox-view`,
      },

      // 2. Conversations
      {
        name: "Conversation",
        label: "Conversations",
        icon: "i-lucide-message-circle",
        children: [
          {
            name: "All",
            label: "All Conversations",
            icon: "i-lucide-inbox",
            to: `${acctPrefix}/conversations`,
          },
          {
            name: "Mentions",
            label: "Mentions",
            icon: "i-lucide-at-sign",
            to: `${acctPrefix}/mentions/conversations`,
          },
          {
            name: "Participating",
            label: "Participating",
            icon: "i-lucide-user-round-check",
            to: `${acctPrefix}/participating/conversations`,
          },
          {
            name: "Unattended",
            label: "Unattended",
            icon: "i-lucide-clock-alert",
            to: `${acctPrefix}/unattended/conversations`,
          },
          {
            name: "Folders",
            label: "Folders",
            icon: "i-lucide-folder",
            collapsible: true,
            showTreeLine: true,
            children: [],
          },
          {
            name: "Teams",
            label: "Teams",
            icon: "i-lucide-users",
            collapsible: true,
            showTreeLine: true,
            children: teams.map((team) => ({
              name: `team-${team.id}`,
              label: team.name,
              to: `${acctPrefix}/team/${team.id}`,
            })),
          },
          {
            name: "Channels",
            label: "Channels",
            icon: "i-lucide-mailbox",
            collapsible: true,
            showTreeLine: true,
            children: inboxes.map((inbox) => ({
              name: `inbox-${inbox.id}`,
              label: inbox.name,
              icon: channelIcon(inbox.channel_type),
              to: `${acctPrefix}/inbox/${inbox.id}`,
            })),
          },
          {
            name: "Labels",
            label: "Labels",
            icon: "i-lucide-tag",
            collapsible: true,
            showTreeLine: true,
            children: labels.map((label) => ({
              name: `label-${label.id}`,
              label: label.title,
              icon: (
                <span
                  className="size-2 rounded-sm shrink-0"
                  style={{ backgroundColor: label.color || "#64748b" }}
                />
              ),
              to: `${acctPrefix}/label/${label.title}`,
            })),
          },
        ],
      },

      // 3. Contacts
      {
        name: "Contacts",
        label: "Contacts",
        icon: "i-lucide-contact",
        children: [
          {
            name: "All Contacts",
            label: "All Contacts",
            to: `${acctPrefix}/contacts`,
            exact: true,
          },
          {
            name: "Active",
            label: "Active",
            to: `${acctPrefix}/contacts/active`,
          },
          {
            name: "Segments",
            label: "Segments",
            icon: "i-lucide-group",
            collapsible: true,
            showTreeLine: true,
            children: [],
          },
          {
            name: "Tagged With",
            label: "Tagged with",
            icon: "i-lucide-tag",
            collapsible: true,
            showTreeLine: true,
            children: labels.map((label) => ({
              name: `contact-label-${label.id}`,
              label: label.title,
              icon: (
                <span
                  className="size-2 rounded-sm shrink-0"
                  style={{ backgroundColor: label.color || "#64748b" }}
                />
              ),
              to: `${acctPrefix}/contacts/labels/${label.title}`,
            })),
          },
        ],
      },

      // 4. Companies
      {
        name: "Companies",
        label: "Companies",
        icon: "i-lucide-building-2",
        children: [
          {
            name: "All Companies",
            label: "All Companies",
            to: `${acctPrefix}/companies`,
          },
        ],
      },

      // 5. Reports
      {
        name: "Reports",
        label: "Reports",
        icon: "i-lucide-chart-spline",
        children: [
          {
            name: "Report Overview",
            label: "Overview",
            to: `${acctPrefix}/reports/overview`,
          },
          {
            name: "Report Conversation",
            label: "Conversations",
            to: `${acctPrefix}/reports/conversation`,
          },
          {
            name: "Reports Agent",
            label: "Agents",
            to: `${acctPrefix}/reports/agents_overview`,
          },
          {
            name: "Reports Label",
            label: "Labels",
            to: `${acctPrefix}/reports/labels_overview`,
          },
          {
            name: "Reports Inbox",
            label: "Inbox",
            to: `${acctPrefix}/reports/inboxes_overview`,
          },
          {
            name: "Reports Team",
            label: "Team",
            to: `${acctPrefix}/reports/teams_overview`,
          },
          {
            name: "Reports CSAT",
            label: "CSAT",
            to: `${acctPrefix}/reports/csat`,
          },
          {
            name: "Reports SLA",
            label: "SLA",
            to: `${acctPrefix}/reports/sla`,
          },
          {
            name: "Reports Bot",
            label: "Bot",
            to: `${acctPrefix}/reports/bot`,
          },
        ],
      },

      // 6. Campaigns
      {
        name: "Campaigns",
        label: "Campaigns",
        icon: "i-lucide-megaphone",
        children: [
          {
            name: "Live chat",
            label: "Live Chat",
            to: `${acctPrefix}/campaigns/live_chat`,
          },
          {
            name: "SMS",
            label: "SMS",
            to: `${acctPrefix}/campaigns/sms`,
          },
          {
            name: "WhatsApp",
            label: "WhatsApp",
            to: `${acctPrefix}/campaigns/whatsapp`,
          },
        ],
      },

      // 7. Settings
      {
        name: "Settings",
        label: "Settings",
        icon: "i-lucide-bolt",
        children: [
          {
            name: "Settings Account Settings",
            label: "Account Settings",
            icon: "i-lucide-briefcase",
            to: `${acctPrefix}/settings/general`,
          },
          {
            name: "Settings Agents",
            label: "Agents",
            icon: "i-lucide-square-user",
            to: `${acctPrefix}/settings/agents`,
          },
          {
            name: "Settings Teams",
            label: "Teams",
            icon: "i-lucide-users",
            to: `${acctPrefix}/settings/teams`,
          },
          {
            name: "Settings Agent Assignment",
            label: "Agent Assignment",
            icon: "i-lucide-user-cog",
            to: `${acctPrefix}/settings/assignment-policy`,
          },
          {
            name: "Settings Inboxes",
            label: "Inboxes",
            icon: "i-lucide-inbox",
            to: `${acctPrefix}/settings/inboxes`,
          },
          {
            name: "Settings Templates",
            label: "Templates",
            icon: "i-lucide-layout-template",
            to: `${acctPrefix}/settings/templates`,
          },
          {
            name: "Settings Labels",
            label: "Labels",
            icon: "i-lucide-tags",
            to: `${acctPrefix}/settings/labels`,
          },
          {
            name: "Settings Custom Attributes",
            label: "Custom Attributes",
            icon: "i-lucide-code",
            to: `${acctPrefix}/settings/custom-attributes`,
          },
          {
            name: "Settings Automation",
            label: "Automation",
            icon: "i-lucide-repeat",
            to: `${acctPrefix}/settings/automation`,
          },
          {
            name: "Settings Agent Bots",
            label: "Bots",
            icon: "i-lucide-bot",
            to: `${acctPrefix}/settings/agent-bots`,
          },
          {
            name: "Settings Macros",
            label: "Macros",
            icon: "i-lucide-toy-brick",
            to: `${acctPrefix}/settings/macros`,
          },
          {
            name: "Settings Canned Responses",
            label: "Canned Responses",
            icon: "i-lucide-message-square-quote",
            to: `${acctPrefix}/settings/canned-response`,
          },
          {
            name: "Settings Integrations",
            label: "Integrations",
            icon: "i-lucide-blocks",
            to: `${acctPrefix}/settings/integrations`,
          },
          {
            name: "Settings Data",
            label: "Data",
            icon: "i-lucide-database",
            to: `${acctPrefix}/settings/data`,
          },
          {
            name: "Settings Audit Logs",
            label: "Audit Logs",
            icon: "i-lucide-briefcase",
            to: `${acctPrefix}/settings/audit-logs`,
          },
          {
            name: "Settings Custom Roles",
            label: "Custom Roles",
            icon: "i-lucide-shield-plus",
            to: `${acctPrefix}/settings/custom-roles`,
          },
          {
            name: "Settings Sla",
            label: "SLA",
            icon: "i-lucide-clock-alert",
            to: `${acctPrefix}/settings/sla`,
          },
          {
            name: "Conversation Workflow",
            label: "Conversation Workflow",
            icon: "i-lucide-workflow",
            to: `${acctPrefix}/settings/conversation-workflow`,
          },
          {
            name: "Settings Security",
            label: "Security",
            icon: "i-lucide-shield",
            to: `${acctPrefix}/settings/security`,
          },
        ],
      },
    ];
  }, [accountId, inboxes, labels, teams]);

  return (
    <aside
      className="bg-n-background flex flex-col text-sm pb-px relative shrink-0 h-full border-r border-n-weak select-none"
      style={{
        width: `${isCollapsed ? MIN_WIDTH : sidebarWidth}px`,
        transition: isResizing ? "none" : "width 150ms ease-out",
      }}
    >
      {/* Top Header: Account switcher & Search */}
      <section className={`grid ${isCollapsed ? "mt-3 mb-4 gap-3 px-1" : "mt-2 mb-3 gap-2 px-2"}`}>
        <div
          className={`flex items-center min-w-0 ${
            isCollapsed ? "justify-center" : "gap-2"
          }`}
        >
          {isCollapsed ? (
            <SidebarAccountSwitcher
              isCollapsed
              currentAccount={currentAccount}
              accounts={me.accounts}
            />
          ) : (
            <>
              <div className="grid shrink-0 place-content-center size-6">
                <Logo className="size-4" />
              </div>
              <div className="shrink-0 w-px h-3 bg-n-strong" />
              <SidebarAccountSwitcher
                currentAccount={currentAccount}
                accounts={me.accounts}
              />
            </>
          )}
        </div>

        {/* Search & New Conversation */}
        <div
          className={`flex items-center min-w-0 ${
            isCollapsed ? "justify-center" : "gap-1.5"
          }`}
        >
          {isCollapsed ? (
            <Link
              to={`/accounts/${accountId}/search`}
              className="flex items-center justify-center size-8 rounded-lg outline outline-1 outline-n-weak bg-n-button-color hover:bg-n-alpha-2 transition-colors"
              title="Search"
            >
              <Search className="size-4 text-n-slate-11" />
            </Link>
          ) : (
            <>
              <Link
                to={`/accounts/${accountId}/search`}
                className="flex items-center gap-2 px-2 py-1 flex-1 min-w-0 h-7 rounded-lg outline outline-1 outline-n-weak bg-n-button-color text-n-slate-10 hover:text-n-slate-12 transition-colors"
              >
                <Search className="size-3.5 shrink-0 text-n-slate-10" />
                <span className="truncate text-xs flex-1">Search</span>
                <span className="text-[11px] font-mono opacity-60">⌘K</span>
              </Link>
              <Link
                to={`/accounts/${accountId}/conversations`}
                className="flex items-center justify-center size-7 rounded-lg outline outline-1 outline-n-weak bg-n-button-color text-n-slate-11 hover:text-n-slate-12 hover:bg-n-alpha-2 transition-colors shrink-0"
                title="New conversation"
              >
                <PenLine className="size-3.5" />
              </Link>
            </>
          )}
        </div>
      </section>

      {/* Nav Menu Items */}
      <nav
        className={`flex-1 overflow-y-auto no-scrollbar min-w-0 ${
          isCollapsed ? "px-1" : "px-2"
        }`}
      >
        <ul
          className={`flex flex-col gap-1 m-0 list-none min-w-0 ${
            isCollapsed ? "items-center" : ""
          }`}
        >
          {menuItems.map((item) => (
            <SidebarGroup
              key={item.name}
              name={item.name}
              label={item.label}
              icon={item.icon}
              to={item.to}
              children={item.children}
              isCollapsed={isCollapsed}
              isExpanded={expandedGroup === item.name}
              onToggle={() =>
                setExpandedGroup(expandedGroup === item.name ? null : item.name)
              }
              activePath={location.pathname}
            />
          ))}
        </ul>
      </nav>

      {/* Bottom Profile section */}
      <section className="mt-auto shrink-0 border-t border-n-weak p-1">
        <SidebarProfileMenu
          me={me}
          accountId={accountId}
          isCollapsed={isCollapsed}
        />
      </section>

      {/* Desktop Resize Handle */}
      <div
        className="hidden md:block absolute top-0 right-0 h-full w-1.5 cursor-col-resize z-40 group hover:bg-n-brand/30"
        onMouseDown={startResize}
        onDoubleClick={toggleCollapsed}
        title="Double click to toggle collapse, drag to resize"
      >
        <div
          className={`absolute top-0 right-0 h-full w-0.5 transition-colors ${
            isResizing ? "bg-n-brand" : "group-hover:bg-n-brand"
          }`}
        />
      </div>
    </aside>
  );
}
