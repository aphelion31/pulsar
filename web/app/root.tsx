import { useEffect } from "react";
import { isRouteErrorResponse, Links, Meta, Outlet, Scripts, ScrollRestoration } from "react-router";
import type { Route } from "./+types/root";
import { initLocale, useLocale } from "./lib/i18n";
import { initThemeListener } from "./lib/theme";
import "./app.css";

const THEME_INIT_SCRIPT = `(function(){try{var t=localStorage.getItem("pulsar.theme");var d=t==="dark"||(t==="auto"&&window.matchMedia("(prefers-color-scheme: dark)").matches);if(d){document.documentElement.classList.add("dark");document.documentElement.style.colorScheme="dark";}else{document.documentElement.style.colorScheme="light";}}catch(e){}})();`;

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>Pulsar</title>
        <link rel="icon" type="image/svg+xml" href="/app/brand-assets/logo_thumbnail.svg" />
        <link rel="icon" type="image/png" sizes="32x32" href="/app/favicon-32x32.png" />
        <link rel="shortcut icon" href="/favicon.ico" />
        <link rel="apple-touch-icon" sizes="180x180" href="/app/apple-touch-icon.png" />
        <script dangerouslySetInnerHTML={{ __html: THEME_INIT_SCRIPT }} />
        <Meta />
        <Links />
      </head>
      <body className="bg-n-background text-n-slate-12 antialiased">
        {children}
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

export default function App() {
  const locale = useLocale();
  useEffect(() => {
    initLocale();
    return initThemeListener();
  }, []);
  return <Outlet key={locale} />;
}

export function HydrateFallback() {
  return (
    <div className="grid h-dvh place-items-center bg-n-background text-sm text-n-slate-11">
      Loading
    </div>
  );
}

export function ErrorBoundary({ error }: Route.ErrorBoundaryProps) {
  const title = isRouteErrorResponse(error) ? `${error.status}` : "Something went wrong";
  const detail = isRouteErrorResponse(error) ? error.statusText : error instanceof Error ? error.message : "";
  return (
    <main className="mx-auto max-w-xl p-8 text-n-slate-12">
      <h1 className="text-lg font-medium">{title}</h1>
      <p className="mt-2 text-n-slate-11">{detail}</p>
    </main>
  );
}
