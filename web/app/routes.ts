import { type RouteConfig, index, layout, route } from "@react-router/dev/routes";

export default [
  route("login", "routes/login.tsx"),
  route("widget", "routes/widget.tsx"),
  route("dev/components", "routes/dev.components.tsx"),
  layout("routes/shell.tsx", [
    index("routes/index.tsx"),
    route("accounts/:accountId/conversations/:conversationId?", "routes/inbox.tsx"),
    route("accounts/:accountId/contacts", "routes/contacts.tsx"),
    route("accounts/:accountId/settings/:section?", "routes/settings.tsx"),
  ]),
  route("*", "routes/not-found.tsx"),
] satisfies RouteConfig;
