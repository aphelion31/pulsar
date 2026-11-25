import { Bot, FileText, MessageSquareQuote, Settings, Wrench } from "lucide-react";
import { Link } from "react-router";

export function CaptainNav({ accountId, active }: { accountId: number; active: string }) {
  const tabs = [
    { key: "assistants", label: "Assistants", to: `/accounts/${accountId}/captain/assistants`, icon: Bot },
    { key: "documents", label: "Documents", to: `/accounts/${accountId}/captain/documents`, icon: FileText },
    { key: "responses", label: "FAQs & Suggestions", to: `/accounts/${accountId}/captain/responses`, icon: MessageSquareQuote },
    { key: "tools", label: "Tools & MCP", to: `/accounts/${accountId}/captain/tools`, icon: Wrench },
    { key: "preferences", label: "Preferences", to: `/accounts/${accountId}/captain/preferences`, icon: Settings },
  ];
  return (
    <div className="border-b border-line bg-surface px-6">
      <div className="mx-auto flex max-w-6xl items-center justify-between gap-4 py-3">
        <div className="flex items-center gap-2.5">
          <span className="grid size-7 place-items-center rounded-lg bg-n-blue-3 text-n-blue-11">
            <Bot size={16} />
          </span>
          <h1 className="font-inter-display text-lg font-semibold text-n-slate-12">Captain</h1>
        </div>
        <nav className="flex flex-wrap gap-1">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = active === tab.key;
            return (
              <Link
                key={tab.key}
                to={tab.to}
                className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors ${
                  isActive
                    ? "bg-n-brand text-white"
                    : "text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
                }`}
              >
                <Icon size={14} />
                {tab.label}
              </Link>
            );
          })}
        </nav>
      </div>
    </div>
  );
}
