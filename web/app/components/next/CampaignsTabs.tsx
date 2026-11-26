import { NavLink } from "react-router";

export function CampaignsTabs({ accountId }: { accountId: number }) {
  const base = `/accounts/${accountId}/campaigns`;
  const tabs = [
    { to: `${base}/live_chat`, label: "Live Chat" },
    { to: `${base}/sms`, label: "SMS" },
    { to: `${base}/whatsapp`, label: "WhatsApp" },
  ];
  return (
    <nav aria-label="Campaign channels" className="mb-6 flex flex-wrap gap-2 border-b border-n-weak pb-3">
      {tabs.map((tab) => (
        <NavLink
          key={tab.to}
          to={tab.to}
          className={({ isActive }) =>
            `rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
              isActive
                ? "bg-n-blue-3 text-n-blue-11 dark:bg-n-alpha-2 dark:text-n-slate-12"
                : "text-n-slate-11 hover:bg-n-alpha-1 hover:text-n-slate-12"
            }`
          }
        >
          {tab.label}
        </NavLink>
      ))}
    </nav>
  );
}
