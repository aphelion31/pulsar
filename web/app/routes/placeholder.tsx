import { useLocation } from "react-router";
import { PageHeader } from "~/components/ui";

const PATH_TITLES: Array<[RegExp, string]> = [
  // Reports
  [/\/reports\/overview/, "Overview"],
  [/\/reports\/conversation/, "Conversations"],
  [/\/reports\/agents?/, "Agents"],
  [/\/reports\/inboxes?/, "Inbox"],
  [/\/reports\/labels?/, "Labels"],
  [/\/reports\/teams?/, "Team"],
  [/\/reports\/sla/, "SLA"],
  [/\/reports\/csat/, "CSAT"],
  [/\/reports\/bot/, "Bot"],
  [/\/reports\/monitors/, "Monitors"],
  [/\/reports/, "Reports"],

  // Campaigns
  [/\/campaigns\/live_chat/, "Live Chat"],
  [/\/campaigns\/sms/, "SMS"],
  [/\/campaigns\/whatsapp\/new/, "New Campaign"],
  [/\/campaigns\/whatsapp\/[^/]+\/analytics/, "Campaign Analytics"],
  [/\/campaigns\/whatsapp/, "WhatsApp"],
  [/\/campaigns\/ongoing/, "Ongoing"],
  [/\/campaigns\/one_off/, "One off"],
  [/\/campaigns/, "Campaigns"],

  // Calls
  [/\/calls/, "Calls"],

  // Companies
  [/\/companies/, "Companies"],

  // Contacts sub-routes
  [/\/contacts\/segments/, "Segments"],
  [/\/contacts\/labels/, "Tagged with"],
  [/\/contacts\/active/, "Active"],
  [/\/contacts\/[^/]+/, "Contact details"],

  // Inbox View
  [/\/inbox-view/, "Inbox View"],

  // Conversations sub-views
  [/\/dashboard/, "Conversations"],
  [/\/mentions\/conversations/, "Mentions"],
  [/\/participating\/conversations/, "Participating"],
  [/\/unattended\/conversations/, "Unattended"],
  [/\/custom_view\//, "Folders"],
  [/\/team\/[^/]+/, "Teams"],
  [/\/label\/[^/]+/, "Labels"],
  [/\/inbox\/[^/]+/, "Inbox"],

  // Profile
  [/\/profile\/mfa/, "Multi-factor Authentication"],
  [/\/profile/, "Profile Settings"],

  // Settings
  [/\/settings\/general/, "Account Settings"],
  [/\/settings\/agent-bots/, "Bots"],
  [/\/settings\/assignment-policy/, "Agent Assignment"],
  [/\/settings\/audit-logs/, "Audit Logs"],
  [/\/settings\/automation/, "Automation"],
  [/\/settings\/canned-response/, "Canned Responses"],
  [/\/settings\/conversation-workflow/, "Conversation Workflow"],
  [/\/settings\/custom-attributes/, "Custom Attributes"],
  [/\/settings\/custom-roles/, "Custom Roles"],
  [/\/settings\/data/, "Data"],
  [/\/settings\/integrations/, "Integrations"],
  [/\/settings\/macros/, "Macros"],
  [/\/settings\/security/, "Security"],
  [/\/settings\/sla/, "SLA"],
  [/\/settings\/templates/, "Templates"],
  [/\/settings\/inboxes\/new/, "New inbox"],
  [/\/settings\/inboxes\/[^/]+/, "Inbox Settings"],
  [/\/settings\/teams\/new/, "New team"],
  [/\/settings\/teams\/[^/]+\/edit/, "Edit team"],

  // Search
  [/\/search/, "Search"],

  // Onboarding & Suspended
  [/\/onboarding\/inbox-setup/, "Inbox Setup"],
  [/\/onboarding/, "Onboarding"],
  [/\/suspended/, "Account Suspended"],
  [/\/shopify\/select-account/, "Select Account"],
  [/\/no-accounts/, "No Accounts"],
];

function getTitle(pathname: string): string {
  for (const [regex, title] of PATH_TITLES) {
    if (regex.test(pathname)) return title;
  }
  return "Coming soon";
}

export default function Placeholder() {
  const { pathname } = useLocation();
  const title = getTitle(pathname);

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col">
      <PageHeader title={title} />
      <div className="grid flex-1 place-items-center p-8 text-center">
        <div className="max-w-sm">
          <p className="font-medium text-ink">{title}</p>
          <p className="mt-1 text-ink-3">Coming soon</p>
        </div>
      </div>
    </div>
  );
}
