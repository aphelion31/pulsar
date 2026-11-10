import { isRouteErrorResponse, Links, Meta, Outlet, Scripts, ScrollRestoration } from "react-router";
import type { Route } from "./+types/root";
import "./app.css";

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>Pulsar</title>
        <Meta />
        <Links />
      </head>
      <body>
        {children}
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

export default function App() {
  return <Outlet />;
}

export function HydrateFallback() {
  return <div className="grid h-dvh place-items-center text-ink-3">Loading</div>;
}

export function ErrorBoundary({ error }: Route.ErrorBoundaryProps) {
  const title = isRouteErrorResponse(error) ? `${error.status}` : "Something went wrong";
  const detail = isRouteErrorResponse(error) ? error.statusText : error instanceof Error ? error.message : "";
  return (
    <main className="mx-auto max-w-xl p-8">
      <h1 className="text-lg font-medium">{title}</h1>
      <p className="mt-2 text-ink-2">{detail}</p>
    </main>
  );
}
