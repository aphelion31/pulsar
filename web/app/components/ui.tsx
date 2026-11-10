// Shared UI primitives. One button style family, one radius per level, lucide icons at 16px.
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";
import { initials } from "~/lib/format";

export function Button({ variant = "primary", className = "", ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "quiet" | "danger" }) {
  const v =
    variant === "primary"
      ? "bg-accent text-accent-ink hover:bg-[#1a5fb4]"
      : variant === "danger"
        ? "text-bad hover:bg-[#fdecea]"
        : "text-ink-2 hover:bg-raised hover:text-ink";
  return (
    <button
      {...p}
      className={`inline-flex h-8 items-center gap-2 rounded-lg px-3 text-[13px] font-medium transition-colors duration-150 disabled:opacity-50 ${v} ${className}`}
    />
  );
}

export function Input({ label, className = "", ...p }: InputHTMLAttributes<HTMLInputElement> & { label?: string }) {
  const input = (
    <input
      {...p}
      className={`h-9 w-full rounded-lg border border-line-strong bg-surface px-3 text-[14px] outline-none transition-colors duration-150 placeholder:text-ink-3 focus:border-accent ${className}`}
    />
  );
  if (!label) return input;
  return (
    <label className="grid gap-1.5">
      <span className="text-[13px] text-ink-2">{label}</span>
      {input}
    </label>
  );
}

export function Avatar({ name, url, size = 32 }: { name: string | null | undefined; url?: string; size?: number }) {
  if (url) return <img src={url} alt="" width={size} height={size} className="shrink-0 rounded-full object-cover" style={{ width: size, height: size }} />;
  return (
    <span
      aria-hidden
      className="grid shrink-0 place-items-center rounded-full bg-raised text-[11px] font-medium text-ink-2"
      style={{ width: size, height: size }}
    >
      {initials(name)}
    </span>
  );
}

export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <header className="flex h-14 shrink-0 items-center justify-between border-b border-line px-6">
      <h1 className="text-[15px] font-medium">{title}</h1>
      <div className="flex items-center gap-2">{children}</div>
    </header>
  );
}

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="grid h-full place-items-center p-8 text-center">
      <div className="max-w-sm">
        <p className="font-medium text-ink">{title}</p>
        {children && <div className="mt-1 text-ink-3">{children}</div>}
      </div>
    </div>
  );
}

export function ErrorText({ children }: { children: ReactNode }) {
  return children ? (
    <p role="alert" className="text-[13px] text-bad">
      {children}
    </p>
  ) : null;
}
