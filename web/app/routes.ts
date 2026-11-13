import { type RouteConfig, index, layout, route } from "@react-router/dev/routes";

const ph = (path: string, id: string) => route(path, "routes/placeholder.tsx", { id });

export default [
  route("login", "routes/login.tsx"),
  route("auth/reset/password", "routes/auth.reset.tsx"),
  route("auth/password/edit", "routes/auth.password-edit.tsx"),
  route("widget", "routes/widget.tsx"),
  ph("shopify/select-account", "shopify-select-account"),
  route("no-accounts", "routes/settings/r1/no-accounts.tsx", { id: "no-accounts" }),
  route("dev/components", "routes/dev.components.tsx"),

  layout("routes/shell.tsx", [
    index("routes/index.tsx"),

    // Working Pulsar core routes
    route("accounts/:accountId/conversations/:conversationId?", "routes/inbox.tsx"),
    route("accounts/:accountId/contacts", "routes/contacts.tsx"),
    route("accounts/:accountId/settings/:section?", "routes/settings.tsx"),

    // Chatwoot dashboard routes (placeholder)
    // conversation.routes.js: `dashboard` (name: home) renders ConversationView
    route("accounts/:accountId/dashboard", "routes/inbox.tsx", { id: "cw-dashboard" }),
    ph("accounts/:accountId/inbox-view", "cw-inbox-view"),
    ph("accounts/:accountId/inbox-view/:type/:id", "cw-inbox-view-item"),
    ph("accounts/:accountId/calls", "cw-calls"),

    // Conversation sub-routes
    route("accounts/:accountId/inbox/:inbox_id", "routes/inbox.tsx", { id: "cw-inbox-item" }),
    route("accounts/:accountId/inbox/:inbox_id/conversations/:conversation_id", "routes/inbox.tsx", { id: "cw-inbox-item-conv" }),
    route("accounts/:accountId/label/:label", "routes/inbox.tsx", { id: "cw-label-convs" }),
    route("accounts/:accountId/label/:label/conversations/:conversation_id", "routes/inbox.tsx", { id: "cw-label-conv" }),
    route("accounts/:accountId/team/:teamId", "routes/inbox.tsx", { id: "cw-team-convs" }),
    route("accounts/:accountId/team/:teamId/conversations/:conversationId", "routes/inbox.tsx", { id: "cw-team-conv" }),
    route("accounts/:accountId/custom_view/:id", "routes/r2/custom-view.tsx", { id: "cw-custom-view-convs" }),
    route("accounts/:accountId/custom_view/:id/conversations/:conversation_id", "routes/r2/custom-view.tsx", { id: "cw-custom-view-conv" }),
    route("accounts/:accountId/mentions/conversations", "routes/inbox.tsx", { id: "cw-mentions" }),
    route("accounts/:accountId/mentions/conversations/:conversationId", "routes/inbox.tsx", { id: "cw-mentions-conv" }),
    route("accounts/:accountId/unattended/conversations", "routes/inbox.tsx", { id: "cw-unattended" }),
    route("accounts/:accountId/unattended/conversations/:conversationId", "routes/inbox.tsx", { id: "cw-unattended-conv" }),
    route("accounts/:accountId/participating/conversations", "routes/inbox.tsx", { id: "cw-participating" }),
    route("accounts/:accountId/participating/conversations/:conversationId", "routes/inbox.tsx", { id: "cw-participating-conv" }),

    // Contacts sub-routes
    route("accounts/:accountId/contacts/segments/:segmentId", "routes/contacts.tsx", { id: "cw-contacts-segments" }),
    route("accounts/:accountId/contacts/labels/:label", "routes/contacts.tsx", { id: "cw-contacts-labels" }),
    route("accounts/:accountId/contacts/active", "routes/contacts.tsx", { id: "cw-contacts-active" }),
    route("accounts/:accountId/contacts/:contactId", "routes/contacts.detail.tsx"),
    route("accounts/:accountId/contacts/:contactId/segments/:segmentId", "routes/contacts.detail.tsx", { id: "cw-contact-detail-segment" }),
    route("accounts/:accountId/contacts/:contactId/labels/:label", "routes/contacts.detail.tsx", { id: "cw-contact-detail-label" }),

    // Companies
    route("accounts/:accountId/companies", "routes/companies.tsx"),
    route("accounts/:accountId/companies/:companyId", "routes/companies.detail.tsx"),

    // Search
    route("accounts/:accountId/search/:tab?", "routes/r2/search.tsx", { id: "cw-search" }),

    // Campaigns
    route("accounts/:accountId/campaigns", "routes/campaigns.tsx", { id: "cw-campaigns" }),
    route("accounts/:accountId/campaigns/ongoing", "routes/campaigns.tsx", { id: "cw-campaigns-ongoing" }),
    route("accounts/:accountId/campaigns/one_off", "routes/r2/campaigns-one-off.tsx", { id: "cw-campaigns-one-off" }),
    route("accounts/:accountId/campaigns/live_chat", "routes/campaigns.tsx", { id: "cw-campaigns-live-chat" }),
    route("accounts/:accountId/campaigns/sms", "routes/r2/campaigns-one-off.tsx", { id: "cw-campaigns-sms" }),
    ph("accounts/:accountId/campaigns/whatsapp", "cw-campaigns-whatsapp"),
    ph("accounts/:accountId/campaigns/whatsapp/new", "cw-campaigns-whatsapp-new"),
    ph("accounts/:accountId/campaigns/whatsapp/:campaignId/analytics", "cw-campaigns-whatsapp-analytics"),
    ph("accounts/:accountId/campaigns/whatsapp/:campaignId", "cw-campaigns-whatsapp-edit"),

    // Reports
    route("accounts/:accountId/reports", "routes/reports.tsx", { id: "cw-reports" }),
    route("accounts/:accountId/reports/overview", "routes/reports.tsx", { id: "cw-reports-overview" }),
    route("accounts/:accountId/reports/conversation", "routes/reports.tsx", { id: "cw-reports-conversation" }),
    route("accounts/:accountId/reports/agent", "routes/reports.tsx", { id: "cw-reports-agent" }),
    route("accounts/:accountId/reports/agents_overview", "routes/reports.tsx", { id: "cw-reports-agents-overview" }),
    route("accounts/:accountId/reports/agents/:id", "routes/reports.tsx", { id: "cw-reports-agent-detail" }),
    route("accounts/:accountId/reports/inboxes", "routes/reports.tsx", { id: "cw-reports-inboxes" }),
    route("accounts/:accountId/reports/inboxes_overview", "routes/reports.tsx", { id: "cw-reports-inboxes-overview" }),
    route("accounts/:accountId/reports/inboxes/:id", "routes/reports.tsx", { id: "cw-reports-inbox-detail" }),
    route("accounts/:accountId/reports/label", "routes/reports.tsx", { id: "cw-reports-label" }),
    route("accounts/:accountId/reports/labels_overview", "routes/reports.tsx", { id: "cw-reports-labels-overview" }),
    route("accounts/:accountId/reports/labels/:id", "routes/reports.tsx", { id: "cw-reports-label-detail" }),
    route("accounts/:accountId/reports/teams", "routes/reports.tsx", { id: "cw-reports-teams" }),
    route("accounts/:accountId/reports/teams_overview", "routes/reports.tsx", { id: "cw-reports-teams-overview" }),
    route("accounts/:accountId/reports/teams/:id", "routes/reports.tsx", { id: "cw-reports-team-detail" }),
    ph("accounts/:accountId/reports/sla", "cw-reports-sla"),
    route("accounts/:accountId/reports/csat", "routes/reports.tsx", { id: "cw-reports-csat" }),
    route("accounts/:accountId/reports/bot", "routes/reports.tsx", { id: "cw-reports-bot" }),
    ph("accounts/:accountId/reports/monitors", "cw-reports-monitors"),
    ph("accounts/:accountId/reports/monitors/:monitorId", "cw-reports-monitor-detail"),

    // Profile
    route("accounts/:accountId/profile", "routes/settings/profile.tsx", { id: "cw-profile" }),
    route("accounts/:accountId/profile/settings", "routes/settings/profile.tsx", { id: "cw-profile-settings" }),
    route("accounts/:accountId/profile/mfa", "routes/settings/r1/profile-mfa.tsx", { id: "cw-profile-mfa" }),

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
    route("accounts/:accountId/settings/custom-attributes", "routes/settings/attributes.tsx", { id: "cw-settings-custom-attributes" }),
    route("accounts/:accountId/settings/custom-attributes/list", "routes/settings/attributes.tsx"),
    route("accounts/:accountId/settings/automation", "routes/settings/automation.tsx", { id: "cw-settings-automation" }),
    route("accounts/:accountId/settings/automation/list", "routes/settings/automation.tsx", { id: "cw-settings-automation-list" }),
    route("accounts/:accountId/settings/audit-logs", "routes/settings/auditlogs.tsx", { id: "cw-settings-audit-logs" }),
    route("accounts/:accountId/settings/audit-logs/list", "routes/settings/auditlogs.tsx", { id: "cw-settings-audit-logs-list" }),
    route("accounts/:accountId/settings/canned-response", "routes/settings/canned.tsx", { id: "cw-settings-canned-response" }),
    route("accounts/:accountId/settings/canned-response/list", "routes/settings/canned.tsx", { id: "cw-settings-canned-response-list" }),
    route("accounts/:accountId/settings/inboxes/list", "routes/settings/inboxes.tsx", { id: "cw-settings-inboxes-list" }),
    route("accounts/:accountId/settings/inboxes/new", "routes/settings/inbox-new.tsx", { id: "cw-settings-inboxes-new" }),
    route("accounts/:accountId/settings/inboxes/new/:inbox_id/finish", "routes/settings/inbox-new.tsx", { id: "cw-settings-inboxes-finish" }),
    route("accounts/:accountId/settings/inboxes/new/:sub_page", "routes/settings/inbox-new.tsx", { id: "cw-settings-inboxes-subpage" }),
    route("accounts/:accountId/settings/inboxes/new/:inbox_id/agents", "routes/settings/inbox-new.tsx", { id: "cw-settings-inboxes-add-agents" }),
    route("accounts/:accountId/settings/inboxes/:inboxId/:tab?", "routes/settings/inboxes.tsx", { id: "cw-settings-inboxes-show" }),
    ph("accounts/:accountId/settings/templates", "cw-settings-templates"),
    route("accounts/:accountId/settings/integrations", "routes/settings/r1/integrations.tsx", { id: "cw-settings-integrations" }),
    route("accounts/:accountId/settings/integrations/dashboard_apps", "routes/settings/r1/dashboard-apps.tsx", { id: "cw-settings-integrations-dashboard-apps" }),
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
    route("accounts/:accountId/settings/security", "routes/settings/r1/security-page.tsx", { id: "cw-settings-security" }),
    route("accounts/:accountId/settings/conversation-workflow", "routes/settings/r1/conversation-workflow.tsx", { id: "cw-settings-conversation-workflow" }),

    // Onboarding & Suspended
    route("accounts/:accountId/onboarding", "routes/r2/onboarding.tsx", { id: "cw-onboarding" }),
    route("accounts/:accountId/onboarding/inbox-setup", "routes/r2/onboarding-inbox-setup.tsx", { id: "cw-onboarding-inbox-setup" }),
    route("accounts/:accountId/suspended", "routes/settings/r1/suspended.tsx", { id: "cw-suspended" }),
  ]),

  route("*", "routes/not-found.tsx"),
] satisfies RouteConfig;
