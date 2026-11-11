// Small formatting helpers shared by screens.
export function timeAgo(unix: number | null | undefined): string {
  if (!unix) return "";
  const s = Math.max(0, Math.floor(Date.now() / 1000 - unix));
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  if (s < 86400 * 7) return `${Math.floor(s / 86400)}d`;
  return new Date(unix * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function clock(unix: number): string {
  return new Date(unix * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

// Chatwoot PR #13233/#14229/#14595: a phone-number name ("+44 7700 900123") shows its last two digits,
// not "+7". Names start with a letter elsewhere, so a leading symbol is skipped.
export function initials(name: string | null | undefined): string {
  const n = (name ?? "").trim();
  if (/^\+?[\d\s()\-.]{6,}$/.test(n)) return n.replace(/\D/g, "").slice(-2);
  const parts = n.split(/\s+/).map((p) => p.replace(/^[^\p{L}\p{N}]+/u, "")).filter(Boolean);
  return ((parts[0]?.[0] ?? "?") + (parts[1]?.[0] ?? "")).toUpperCase();
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
