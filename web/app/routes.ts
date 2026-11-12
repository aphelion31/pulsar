import { type RouteConfig, index, layout, route } from "@react-router/dev/routes";

const ph = (path: string, id: string) => route(path, "routes/placeholder.tsx", { id });

export default [
  route("login", "routes/login.tsx"),
  route("widget", "routes/widget.tsx"),
  ph("shopify/select-account", "shopify-select-account"),
  ph("no-accounts", "no-accounts"),
  route("dev/components", "routes/dev.components.tsx"),

  layout("routes/shell.tsx", [
    index("routes/index.tsx"),

    // Working Pulsar core routes
    route("accounts/:accountId/conversations/:conversationId?", "routes/inbox.tsx"),
    route("accounts/:accountId/contacts", "routes/contacts.tsx"),
    route("accounts/:accountId/settings/:section?", "routes/settings.tsx"),

    // Chatwoot dashboard routes (placeholder)
    ph("accounts/:accountId/dashboard", "cw-dashboard"),
    ph("accounts/:accountId/inbox-view", "cw-inbox-view"),
    ph("accounts/:accountId/inbox-view/:type/:id", "cw-inbox-view-item"),
    ph("accounts/:accountId/calls", "cw-calls"),

    // Conversation sub-routes
    ph("accounts/:accountId/inbox/:inbox_id", "cw-inbox-item"),
    ph("accounts/:accountId/inbox/:inbox_id/conversations/:conversation_id", "cw-inbox-item-conv"),
    ph("accounts/:accountId/label/:label", "cw-label-convs"),
    ph("accounts/:accountId/label/:label/conversations/:conversation_id", "cw-label-conv"),
    ph("accounts/:accountId/team/:teamId", "cw-team-convs"),
    ph("accounts/:accountId/team/:teamId/conversations/:conversationId", "cw-team-conv"),
    ph("accounts/:accountId/custom_view/:id", "cw-custom-view-convs"),
    ph("accounts/:accountId/custom_view/:id/conversations/:conversation_id", "cw-custom-view-conv"),
    ph("accounts/:accountId/mentions/conversations", "cw-mentions"),
    ph("accounts/:accountId/mentions/conversations/:conversationId", "cw-mentions-conv"),
    ph("accounts/:accountId/unattended/conversations", "cw-unattended"),
    ph("accounts/:accountId/unattended/conversations/:conversationId", "cw-unattended-conv"),
    ph("accounts/:accountId/participating/conversations", "cw-participating"),
    ph("accounts/:accountId/participating/conversations/:conversationId", "cw-participating-conv"),

    // Contacts sub-routes
    ph("accounts/:accountId/contacts/segments/:segmentId", "cw-contacts-segments"),
    ph("accounts/:accountId/contacts/labels/:label", "cw-contacts-labels"),
    ph("accounts/:accountId/contacts/active", "cw-contacts-active"),
    route("accounts/:accountId/contacts/:contactId", "routes/contacts.detail.tsx"),
    ph("accounts/:accountId/contacts/:contactId/segments/:segmentId", "cw-contact-detail-segment"),
    ph("accounts/:accountId/contacts/:contactId/labels/:label", "cw-contact-detail-label"),

    // Companies
    route("accounts/:accountId/companies", "routes/companies.tsx"),
    route("accounts/:accountId/companies/:companyId", "routes/companies.detail.tsx"),

    // Search
    ph("accounts/:accountId/search/:tab?", "cw-search"),

    // Campaigns
    route("accounts/:accountId/campaigns", "routes/campaigns.tsx", { id: "cw-campaigns" }),
    ph("accounts/:accountId/campaigns/ongoing", "cw-campaigns-ongoing"),
    ph("accounts/:accountId/campaigns/one_off", "cw-campaigns-one-off"),
    route("accounts/:accountId/campaigns/live_chat", "routes/campaigns.tsx", { id: "cw-campaigns-live-chat" }),
    ph("accounts/:accountId/campaigns/sms", "cw-campaigns-sms"),
    ph("accounts/:accountId/campaigns/whatsapp", "cw-campaigns-whatsapp"),
    ph("accounts/:accountId/campaigns/whatsapp/new", "cw-campaigns-whatsapp-new"),
    ph("accounts/:accountId/campaigns/whatsapp/:campaignId/analytics", "cw-campaigns-whatsapp-analytics"),
    ph("accounts/:accountId/campaigns/whatsapp/:campaignId", "cw-campaigns-whatsapp-edit"),

    // Reports
    ph("accounts/:accountId/reports", "cw-reports"),
    ph("accounts/:accountId/reports/overview", "cw-reports-overview"),
    ph("accounts/:accountId/reports/conversation", "cw-reports-conversation"),
    ph("accounts/:accountId/reports/agent", "cw-reports-agent"),
    ph("accounts/:accountId/reports/agents_overview", "cw-reports-agents-overview"),
    ph("accounts/:accountId/reports/agents/:id", "cw-reports-agent-detail"),
    ph("accounts/:accountId/reports/inboxes", "cw-reports-inboxes"),
    ph("accounts/:accountId/reports/inboxes_overview", "cw-reports-inboxes-overview"),
    ph("accounts/:accountId/reports/inboxes/:id", "cw-reports-inbox-detail"),
    ph("accounts/:accountId/reports/label", "cw-reports-label"),
    ph("accounts/:accountId/reports/labels_overview", "cw-reports-labels-overview"),
    ph("accounts/:accountId/reports/labels/:id", "cw-reports-label-detail"),
    ph("accounts/:accountId/reports/teams", "cw-reports-teams"),
    ph("accounts/:accountId/reports/teams_overview", "cw-reports-teams-overview"),
    ph("accounts/:accountId/reports/teams/:id", "cw-reports-team-detail"),
    ph("accounts/:accountId/reports/sla", "cw-reports-sla"),
    ph("accounts/:accountId/reports/csat", "cw-reports-csat"),
    ph("accounts/:accountId/reports/bot", "cw-reports-bot"),
    ph("accounts/:accountId/reports/monitors", "cw-reports-monitors"),
    ph("accounts/:accountId/reports/monitors/:monitorId", "cw-reports-monitor-detail"),

    // Profile
    route("accounts/:accountId/profile", "routes/settings/profile.tsx", { id: "cw-profile" }),
    route("accounts/:accountId/profile/settings", "routes/settings/profile.tsx", { id: "cw-profile-settings" }),
    ph("accounts/:accountId/profile/mfa", "cw-profile-mfa"),

    // Settings sub-routes
    route("accounts/:accountId/settings/general", "routes/settings/account.tsx"),
    route("accounts/:accountId/settings/agents/list", "routes/settings/agents.tsx"),
    ph("accounts/:accountId/settings/assignment-policy", "cw-settings-assignment-policy"),
    ph("accounts/:accountId/settings/assignment-policy/index", "cw-settings-assignment-policy-index"),
    ph("accounts/:accountId/settings/assignment-policy/assignment", "cw-settings-assignment-policy-assignment"),
    ph("accounts/:accountId/settings/assignment-policy/assignment/create", "cw-settings-assignment-policy-create"),
    ph("accounts/:accountId/settings/assignment-policy/assignment/edit/:id", "cw-settings-assignment-policy-edit"),
    ph("accounts/:accountId/settings/assignment-policy/capacity", "cw-settings-assignment-policy-capacity"),
    ph("accounts/:accountId/settings/assignment-policy/capacity/create", "cw-settings-assignment-policy-capacity-create"),
    ph("accounts/:accountId/settings/assignment-policy/capacity/edit/:id", "cw-settings-assignment-policy-capacity-edit"),
    route("accounts/:accountId/settings/agent-bots", "routes/settings/agent-bots.tsx", { id: "cw-settings-agent-bots" }),
    ph("accounts/:accountId/settings/custom-attributes", "cw-settings-custom-attributes"),
    route("accounts/:accountId/settings/custom-attributes/list", "routes/settings/attributes.tsx"),
    route("accounts/:accountId/settings/automation", "routes/settings/automation.tsx", { id: "cw-settings-automation" }),
    route("accounts/:accountId/settings/automation/list", "routes/settings/automation.tsx", { id: "cw-settings-automation-list" }),
    route("accounts/:accountId/settings/audit-logs", "routes/settings/auditlogs.tsx", { id: "cw-settings-audit-logs" }),
    route("accounts/:accountId/settings/audit-logs/list", "routes/settings/auditlogs.tsx", { id: "cw-settings-audit-logs-list" }),
    route("accounts/:accountId/settings/canned-response", "routes/settings/canned.tsx", { id: "cw-settings-canned-response" }),
    route("accounts/:accountId/settings/canned-response/list", "routes/settings/canned.tsx", { id: "cw-settings-canned-response-list" }),
    route("accounts/:accountId/settings/inboxes/list", "routes/settings/inboxes.tsx", { id: "cw-settings-inboxes-list" }),
    ph("accounts/:accountId/settings/inboxes/new", "cw-settings-inboxes-new"),
    ph("accounts/:accountId/settings/inboxes/new/:inbox_id/finish", "cw-settings-inboxes-finish"),
    ph("accounts/:accountId/settings/inboxes/new/:sub_page", "cw-settings-inboxes-subpage"),
    ph("accounts/:accountId/settings/inboxes/new/:inbox_id/agents", "cw-settings-inboxes-add-agents"),
    route("accounts/:accountId/settings/inboxes/:inboxId/:tab?", "routes/settings/inboxes.tsx", { id: "cw-settings-inboxes-show" }),
    ph("accounts/:accountId/settings/templates", "cw-settings-templates"),
    ph("accounts/:accountId/settings/integrations", "cw-settings-integrations"),
    ph("accounts/:accountId/settings/integrations/dashboard_apps", "cw-settings-integrations-dashboard-apps"),
    route("accounts/:accountId/settings/integrations/webhook", "routes/settings/webhooks.tsx"),
    ph("accounts/:accountId/settings/integrations/slack", "cw-settings-integrations-slack"),
    ph("accounts/:accountId/settings/integrations/linear", "cw-settings-integrations-linear"),
    ph("accounts/:accountId/settings/integrations/stripe", "cw-settings-integrations-stripe"),
    ph("accounts/:accountId/settings/integrations/google_translate", "cw-settings-integrations-google-translate"),
    ph("accounts/:accountId/settings/integrations/notion", "cw-settings-integrations-notion"),
    ph("accounts/:accountId/settings/integrations/shopify", "cw-settings-integrations-shopify"),
    ph("accounts/:accountId/settings/integrations/:integration_id", "cw-settings-integrations-detail"),
    ph("accounts/:accountId/settings/data", "cw-settings-data"),
    ph("accounts/:accountId/settings/data/:dataImportId", "cw-settings-data-detail"),
    route("accounts/:accountId/settings/labels/list", "routes/settings/labels.tsx"),
    route("accounts/:accountId/settings/macros", "routes/settings/macros.tsx"),
    route("accounts/:accountId/settings/macros/:macroId/edit", "routes/settings/macro-editor.tsx", { id: "cw-settings-macros-edit" }),
    route("accounts/:accountId/settings/macros/new", "routes/settings/macro-editor.tsx", { id: "cw-settings-macros-new" }),
    route("accounts/:accountId/settings/sla", "routes/settings/sla.tsx", { id: "cw-settings-sla" }),
    route("accounts/:accountId/settings/sla/list", "routes/settings/sla.tsx", { id: "cw-settings-sla-list" }),
    route("accounts/:accountId/settings/teams/list", "routes/settings/teams.tsx"),
    route("accounts/:accountId/settings/teams/new", "routes/settings/team-editor.tsx", { id: "cw-settings-teams-new" }),
    route("accounts/:accountId/settings/teams/new/:teamId/finish", "routes/settings/team-editor.tsx", { id: "cw-settings-teams-finish" }),
    route("accounts/:accountId/settings/teams/new/:teamId/agents", "routes/settings/team-editor.tsx", { id: "cw-settings-teams-add-agents" }),
    route("accounts/:accountId/settings/teams/:teamId/edit", "routes/settings/team-editor.tsx", { id: "cw-settings-teams-edit" }),
    route("accounts/:accountId/settings/teams/:teamId/edit/agents", "routes/settings/team-editor.tsx", { id: "cw-settings-teams-edit-agents" }),
    route("accounts/:accountId/settings/teams/:teamId/edit/finish", "routes/settings/team-editor.tsx", { id: "cw-settings-teams-edit-finish" }),
    route("accounts/:accountId/settings/custom-roles", "routes/settings/custom-roles.tsx", { id: "cw-settings-custom-roles" }),
    route("accounts/:accountId/settings/custom-roles/list", "routes/settings/custom-roles.tsx", { id: "cw-settings-custom-roles-list" }),
    ph("accounts/:accountId/settings/security", "cw-settings-security"),
    ph("accounts/:accountId/settings/conversation-workflow", "cw-settings-conversation-workflow"),

    // Onboarding & Suspended
    ph("accounts/:accountId/onboarding", "cw-onboarding"),
    ph("accounts/:accountId/onboarding/inbox-setup", "cw-onboarding-inbox-setup"),
    ph("accounts/:accountId/suspended", "cw-suspended"),
  ]),

  route("*", "routes/not-found.tsx"),
] satisfies RouteConfig;
