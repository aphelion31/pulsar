// Theme preference management matching Chatwoot ("auto" | "light" | "dark").
export type ThemeMode = "auto" | "light" | "dark";

const THEME_KEY = "pulsar.theme";

export function getTheme(): ThemeMode {
  if (typeof window === "undefined") return "light";
  const saved = localStorage.getItem(THEME_KEY);
  if (saved === "light" || saved === "dark" || saved === "auto") return saved;
  return "light";
}

export function isDarkEffective(mode: ThemeMode = getTheme()): boolean {
  if (mode === "dark") return true;
  if (mode === "light") return false;
  if (typeof window === "undefined") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function applyTheme(mode: ThemeMode = getTheme()) {
  if (typeof document === "undefined") return;
  const dark = isDarkEffective(mode);
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

export function setTheme(mode: ThemeMode) {
  if (typeof window === "undefined") return;
  localStorage.setItem(THEME_KEY, mode);
  applyTheme(mode);
  window.dispatchEvent(new CustomEvent("pulsar:theme-changed", { detail: mode }));
}

export function initThemeListener() {
  if (typeof window === "undefined") return () => {};
  applyTheme();
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const onMedia = () => {
    if (getTheme() === "auto") applyTheme("auto");
  };
  mq.addEventListener("change", onMedia);
  return () => mq.removeEventListener("change", onMedia);
}
