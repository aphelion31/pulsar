// Shared centered card + Chatwoot brand header matching v3/views/login/Index.vue.
import type { ReactNode } from "react";

export function AuthShell({
  title,
  subtitle,
  children,
  wiggle = false,
}: {
  title: string;
  subtitle?: ReactNode;
  children: ReactNode;
  wiggle?: boolean;
}) {
  return (
    <main className="flex min-h-dvh w-full flex-col justify-center bg-n-brand/5 px-4 py-12 dark:bg-n-background sm:px-6 lg:px-8">
      <section className="mx-auto w-full max-w-lg text-center">
        <img
          src="/app/brand-assets/logo.svg"
          alt="Chatwoot"
          className="mx-auto block h-8 w-auto dark:hidden"
        />
        <img
          src="/app/brand-assets/logo_dark.svg"
          alt="Chatwoot"
          className="mx-auto hidden h-8 w-auto dark:block"
        />
        <h1 className="mt-6 font-inter-display text-3xl font-medium tracking-tight text-n-slate-12">
          {title}
        </h1>
        {subtitle && (
          <div className="mt-3 text-sm text-n-slate-11">{subtitle}</div>
        )}
      </section>

      <section
        className={`mx-auto mt-10 w-full max-w-lg rounded-xl border border-n-weak bg-white p-8 shadow-lg dark:bg-n-solid-2 sm:p-11 ${
          wiggle ? "animate-wiggle" : ""
        }`}
      >
        {children}
      </section>
    </main>
  );
}
