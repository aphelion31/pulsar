// English strings from Chatwoot's dashboard locale, copied verbatim into ../i18n/en/*.json.
// `t` looks up dotted keys (`GENERAL.CLOSE`). `{name}` placeholders are replaced like vue-i18n;
// unknown placeholders are left as written. Missing keys return the key itself.
import advancedFilters from "../i18n/en/advancedFilters.json";
import agentBots from "../i18n/en/agentBots.json";
import agentMgmt from "../i18n/en/agentMgmt.json";
import attributesMgmt from "../i18n/en/attributesMgmt.json";
import auditLogs from "../i18n/en/auditLogs.json";
import automation from "../i18n/en/automation.json";
import bulkActions from "../i18n/en/bulkActions.json";
import calls from "../i18n/en/calls.json";
import campaign from "../i18n/en/campaign.json";
import cannedMgmt from "../i18n/en/cannedMgmt.json";
import chatlist from "../i18n/en/chatlist.json";
import companies from "../i18n/en/companies.json";
import components from "../i18n/en/components.json";
import contact from "../i18n/en/contact.json";
import contactFilters from "../i18n/en/contactFilters.json";
import contentTemplates from "../i18n/en/contentTemplates.json";
import conversation from "../i18n/en/conversation.json";
import csatMgmt from "../i18n/en/csatMgmt.json";
import customRole from "../i18n/en/customRole.json";
import datePicker from "../i18n/en/datePicker.json";
import emoji from "../i18n/en/emoji.json";
import general from "../i18n/en/general.json";
import generalSettings from "../i18n/en/generalSettings.json";
import helpCenter from "../i18n/en/helpCenter.json";
import inbox from "../i18n/en/inbox.json";
import inboxMgmt from "../i18n/en/inboxMgmt.json";
import integrationApps from "../i18n/en/integrationApps.json";
import integrations from "../i18n/en/integrations.json";
import labelsMgmt from "../i18n/en/labelsMgmt.json";
import login from "../i18n/en/login.json";
import macros from "../i18n/en/macros.json";
import mfa from "../i18n/en/mfa.json";
import onboarding from "../i18n/en/onboarding.json";
import report from "../i18n/en/report.json";
import resetPassword from "../i18n/en/resetPassword.json";
import search from "../i18n/en/search.json";
import sessionLimit from "../i18n/en/sessionLimit.json";
import setNewPassword from "../i18n/en/setNewPassword.json";
import settings from "../i18n/en/settings.json";
import signup from "../i18n/en/signup.json";
import sla from "../i18n/en/sla.json";
import snooze from "../i18n/en/snooze.json";
import teamsSettings from "../i18n/en/teamsSettings.json";
import webhooks from "../i18n/en/webhooks.json";
import whatsappTemplateMgmt from "../i18n/en/whatsappTemplateMgmt.json";
import whatsappTemplates from "../i18n/en/whatsappTemplates.json";
import yearInReview from "../i18n/en/yearInReview.json";

import { useEffect, useState } from "react";
import { SUPPORTED_LOCALES, type LocaleOption } from "./locales.generated";
import { pickPlural, type Vars } from "./plural";
export type { LocaleOption, Vars };
export { SUPPORTED_LOCALES };

const LOCALE_KEY = "pulsar.locale";

const baseMessages: Record<string, unknown> = {
  ...advancedFilters,
  ...agentBots,
  ...agentMgmt,
  ...attributesMgmt,
  ...auditLogs,
  ...automation,
  ...bulkActions,
  ...calls,
  ...campaign,
  ...cannedMgmt,
  ...chatlist,
  ...companies,
  ...components,
  ...contact,
  ...contactFilters,
  ...contentTemplates,
  ...conversation,
  ...csatMgmt,
  ...customRole,
  ...datePicker,
  ...emoji,
  ...general,
  ...generalSettings,
  ...helpCenter,
  ...inbox,
  ...inboxMgmt,
  ...integrationApps,
  ...integrations,
  ...labelsMgmt,
  ...login,
  ...macros,
  ...mfa,
  ...onboarding,
  ...report,
  ...resetPassword,
  ...search,
  ...sessionLimit,
  ...setNewPassword,
  ...settings,
  ...signup,
  ...sla,
  ...snooze,
  ...teamsSettings,
  ...webhooks,
  ...whatsappTemplateMgmt,
  ...whatsappTemplates,
  ...yearInReview,
};

const localeCache = new Map<string, Record<string, unknown>>([["en", baseMessages]]);
let currentLocale = "en";

export function isRtlLocale(code: string): boolean {
  return SUPPORTED_LOCALES.find((l) => l.code === code)?.dir === "rtl";
}

export function getLocale(): string {
  if (typeof window !== "undefined") {
    const saved = localStorage.getItem(LOCALE_KEY);
    if (saved && SUPPORTED_LOCALES.some((l) => l.code === saved)) {
      currentLocale = saved;
    }
  }
  return currentLocale;
}

function applyDocumentLocale(code: string) {
  if (typeof document === "undefined") return;
  document.documentElement.lang = code.replace("_", "-");
  document.documentElement.dir = isRtlLocale(code) ? "rtl" : "ltr";
}

export async function loadLocaleBundle(code: string): Promise<Record<string, unknown>> {
  if (localeCache.has(code)) return localeCache.get(code)!;
  if (typeof fetch === "undefined") return baseMessages;
  try {
    const res = await fetch(`/app/i18n/${encodeURIComponent(code)}.json`);
    if (res.ok) {
      const json = (await res.json()) as Record<string, unknown>;
      localeCache.set(code, json);
      return json;
    }
  } catch {
    // Fallback to English bundle when offline or unavailable.
  }
  return baseMessages;
}

export async function setLocale(code: string): Promise<string> {
  const next = SUPPORTED_LOCALES.some((l) => l.code === code) ? code : "en";
  currentLocale = next;
  if (typeof window !== "undefined") {
    localStorage.setItem(LOCALE_KEY, next);
  }
  await loadLocaleBundle(next);
  applyDocumentLocale(next);
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent("pulsar:locale-changed", { detail: next }));
  }
  return next;
}

export function initLocale() {
  const active = getLocale();
  applyDocumentLocale(active);
  if (active !== "en") {
    void loadLocaleBundle(active).then(() => {
      if (typeof window !== "undefined") {
        window.dispatchEvent(new CustomEvent("pulsar:locale-changed", { detail: active }));
      }
    });
  }
}

export function useLocale(): string {
  const [loc, setLoc] = useState(getLocale);
  useEffect(() => {
    const onChange = () => setLoc(getLocale());
    window.addEventListener("pulsar:locale-changed", onChange);
    return () => window.removeEventListener("pulsar:locale-changed", onChange);
  }, []);
  return loc;
}

function lookupIn(dict: Record<string, unknown>, key: string): string | undefined {
  let node: unknown = dict;
  for (const part of key.split(".")) {
    if (node && typeof node === "object" && part in node) {
      node = (node as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return typeof node === "string" && node.length > 0 ? node : undefined;
}

export function t(key: string, vars?: Vars): string {
  const activeDict = localeCache.get(currentLocale) ?? baseMessages;
  const hit = lookupIn(activeDict, key) ?? lookupIn(baseMessages, key);
  const raw = pickPlural(hit ?? key, vars).replace(/\{'([^']+)'\}/g, "$1");
  if (!vars) return raw;
  return raw.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.prototype.hasOwnProperty.call(vars, name) ? String(vars[name]) : match,
  );
}
