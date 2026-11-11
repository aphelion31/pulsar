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
    ph("accounts/:accountId/contacts/:contactId", "cw-contact-detail"),
    ph("accounts/:accountId/contacts/:contactId/segments/:segmentId", "cw-contact-detail-segment"),
    ph("accounts/:accountId/contacts/:contactId/labels/:label", "cw-contact-detail-label"),

    // Companies
    ph("accounts/:accountId/companies", "cw-companies"),
    ph("accounts/:accountId/companies/:companyId", "cw-company-detail"),

    // Search
    ph("accounts/:accountId/search/:tab?", "cw-search"),

    // Campaigns
    ph("accounts/:accountId/campaigns", "cw-campaigns"),
    ph("accounts/:accountId/campaigns/ongoing", "cw-campaigns-ongoing"),
    ph("accounts/:accountId/campaigns/one_off", "cw-campaigns-one-off"),
    ph("accounts/:accountId/campaigns/live_chat", "cw-campaigns-live-chat"),
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
    ph("accounts/:accountId/profile", "cw-profile"),
    ph("accounts/:accountId/profile/settings", "cw-profile-settings"),
    ph("accounts/:accountId/profile/mfa", "cw-profile-mfa"),

    // Settings sub-routes
    ph("accounts/:accountId/settings/general", "cw-settings-general"),
    ph("accounts/:accountId/settings/agents/list", "cw-settings-agents-list"),
    ph("accounts/:accountId/settings/assignment-policy", "cw-settings-assignment-policy"),
    ph("accounts/:accountId/settings/assignment-policy/index", "cw-settings-assignment-policy-index"),
    ph("accounts/:accountId/settings/assignment-policy/assignment", "cw-settings-assignment-policy-assignment"),
    ph("accounts/:accountId/settings/assignment-policy/assignment/create", "cw-settings-assignment-policy-create"),
    ph("accounts/:accountId/settings/assignment-policy/assignment/edit/:id", "cw-settings-assignment-policy-edit"),
    ph("accounts/:accountId/settings/assignment-policy/capacity", "cw-settings-assignment-policy-capacity"),
    ph("accounts/:accountId/settings/assignment-policy/capacity/create", "cw-settings-assignment-policy-capacity-create"),
    ph("accounts/:accountId/settings/assignment-policy/capacity/edit/:id", "cw-settings-assignment-policy-capacity-edit"),
    ph("accounts/:accountId/settings/agent-bots", "cw-settings-agent-bots"),
    ph("accounts/:accountId/settings/custom-attributes", "cw-settings-custom-attributes"),
    ph("accounts/:accountId/settings/custom-attributes/list", "cw-settings-custom-attributes-list"),
    ph("accounts/:accountId/settings/automation", "cw-settings-automation"),
    ph("accounts/:accountId/settings/automation/list", "cw-settings-automation-list"),
    ph("accounts/:accountId/settings/audit-logs", "cw-settings-audit-logs"),
    ph("accounts/:accountId/settings/audit-logs/list", "cw-settings-audit-logs-list"),
    ph("accounts/:accountId/settings/canned-response", "cw-settings-canned-response"),
    ph("accounts/:accountId/settings/canned-response/list", "cw-settings-canned-response-list"),
    ph("accounts/:accountId/settings/inboxes/list", "cw-settings-inboxes-list"),
    ph("accounts/:accountId/settings/inboxes/new", "cw-settings-inboxes-new"),
    ph("accounts/:accountId/settings/inboxes/new/:inbox_id/finish", "cw-settings-inboxes-finish"),
    ph("accounts/:accountId/settings/inboxes/new/:sub_page", "cw-settings-inboxes-subpage"),
    ph("accounts/:accountId/settings/inboxes/new/:inbox_id/agents", "cw-settings-inboxes-add-agents"),
    ph("accounts/:accountId/settings/inboxes/:inboxId/:tab?", "cw-settings-inboxes-show"),
    ph("accounts/:accountId/settings/templates", "cw-settings-templates"),
    ph("accounts/:accountId/settings/integrations", "cw-settings-integrations"),
    ph("accounts/:accountId/settings/integrations/dashboard_apps", "cw-settings-integrations-dashboard-apps"),
    ph("accounts/:accountId/settings/integrations/webhook", "cw-settings-integrations-webhook"),
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
    ph("accounts/:accountId/settings/macros", "cw-settings-macros"),
    ph("accounts/:accountId/settings/macros/:macroId/edit", "cw-settings-macros-edit"),
    ph("accounts/:accountId/settings/macros/new", "cw-settings-macros-new"),
    ph("accounts/:accountId/settings/sla", "cw-settings-sla"),
    ph("accounts/:accountId/settings/sla/list", "cw-settings-sla-list"),
    ph("accounts/:accountId/settings/teams/list", "cw-settings-teams-list"),
    ph("accounts/:accountId/settings/teams/new", "cw-settings-teams-new"),
    ph("accounts/:accountId/settings/teams/new/:teamId/finish", "cw-settings-teams-finish"),
    ph("accounts/:accountId/settings/teams/new/:teamId/agents", "cw-settings-teams-add-agents"),
    ph("accounts/:accountId/settings/teams/:teamId/edit", "cw-settings-teams-edit"),
    ph("accounts/:accountId/settings/teams/:teamId/edit/agents", "cw-settings-teams-edit-agents"),
    ph("accounts/:accountId/settings/teams/:teamId/edit/finish", "cw-settings-teams-edit-finish"),
    ph("accounts/:accountId/settings/custom-roles", "cw-settings-custom-roles"),
    ph("accounts/:accountId/settings/custom-roles/list", "cw-settings-custom-roles-list"),
    ph("accounts/:accountId/settings/security", "cw-settings-security"),
    ph("accounts/:accountId/settings/conversation-workflow", "cw-settings-conversation-workflow"),

    // Onboarding & Suspended
    ph("accounts/:accountId/onboarding", "cw-onboarding"),
    ph("accounts/:accountId/onboarding/inbox-setup", "cw-onboarding-inbox-setup"),
    ph("accounts/:accountId/suspended", "cw-suspended"),
  ]),

  route("*", "routes/not-found.tsx"),
] satisfies RouteConfig;
