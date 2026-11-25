import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { acct, api, type Agent } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

// mirrors chatwoot/app/javascript/dashboard/helper/auditlogHelper.js
type AuditLog = {
  id: number;
  auditable_id: number | null;
  auditable_type: string;
  associated_type: string | null;
  user_id: number | null;
  action: string;
  audited_changes: Record<string, any>;
  remote_address: string | null;
  created_at: string;
  auditable: { user_id?: number } | null;
};
type Page = { audit_logs: AuditLog[]; current_page: number; per_page: number; total_entries: number };

const translationKeys: Record<string, string> = {
  "automationrule:create": "AUDIT_LOGS.AUTOMATION_RULE.ADD",
  "automationrule:update": "AUDIT_LOGS.AUTOMATION_RULE.EDIT",
  "automationrule:destroy": "AUDIT_LOGS.AUTOMATION_RULE.DELETE",
  "webhook:create": "AUDIT_LOGS.WEBHOOK.ADD",
  "webhook:update": "AUDIT_LOGS.WEBHOOK.EDIT",
  "webhook:destroy": "AUDIT_LOGS.WEBHOOK.DELETE",
  "inbox:create": "AUDIT_LOGS.INBOX.ADD",
  "inbox:update": "AUDIT_LOGS.INBOX.EDIT",
  "inbox:destroy": "AUDIT_LOGS.INBOX.DELETE",
  "user:sign_in": "AUDIT_LOGS.USER_ACTION.SIGN_IN",
  "user:sign_out": "AUDIT_LOGS.USER_ACTION.SIGN_OUT",
  "team:create": "AUDIT_LOGS.TEAM.ADD",
  "team:update": "AUDIT_LOGS.TEAM.EDIT",
  "team:destroy": "AUDIT_LOGS.TEAM.DELETE",
  "macro:create": "AUDIT_LOGS.MACRO.ADD",
  "macro:update": "AUDIT_LOGS.MACRO.EDIT",
  "macro:destroy": "AUDIT_LOGS.MACRO.DELETE",
  "accountuser:create": "AUDIT_LOGS.ACCOUNT_USER.ADD",
  "accountuser:update:self": "AUDIT_LOGS.ACCOUNT_USER.EDIT.SELF",
  "accountuser:update:other": "AUDIT_LOGS.ACCOUNT_USER.EDIT.OTHER",
  "accountuser:update:deleted": "AUDIT_LOGS.ACCOUNT_USER.EDIT.DELETED",
  "inboxmember:create": "AUDIT_LOGS.INBOX_MEMBER.ADD",
  "inboxmember:destroy": "AUDIT_LOGS.INBOX_MEMBER.REMOVE",
  "teammember:create": "AUDIT_LOGS.TEAM_MEMBER.ADD",
  "teammember:destroy": "AUDIT_LOGS.TEAM_MEMBER.REMOVE",
  "account:update": "AUDIT_LOGS.ACCOUNT.EDIT",
  "conversation:destroy": "AUDIT_LOGS.CONVERSATION.DELETE",
  "message:destroy": "AUDIT_LOGS.MESSAGE.DELETE",
};

const roleMapping: Record<number, string> = { 0: "agent", 1: "administrator" };
const availabilityMapping: Record<number, string> = { 0: "online", 1: "offline", 2: "busy" };

const last = (v: any) => (Array.isArray(v) ? v[v.length - 1] : v);

function agentName(id: number | null | undefined, agents: Agent[]) {
  if (id === null || id === undefined) return t("AUDIT_LOGS.DEFAULT_USER");
  return agents.find(a => a.id === id)?.name ?? String(id);
}

function activity(log: AuditLog, agents: Agent[]): string {
  const type = log.auditable_type.toLowerCase();
  const action = log.action.toLowerCase();
  const changes = log.audited_changes ?? {};
  const vars: Record<string, string | number> = { agentName: agentName(log.user_id, agents), id: log.auditable_id ?? "" };

  if (type === "conversation" && action === "destroy") vars.id = changes.display_id ?? log.auditable_id ?? "";
  if (type === "message" && action === "destroy") vars.conversationId = changes.display_id ?? "";

  if (type === "accountuser" && action === "create") {
    vars.invitee = agentName(changes.user_id, agents);
    vars.role = roleMapping[changes.role] ?? "unknown";
  }
  if (type === "accountuser" && action === "update") {
    if (log.user_id !== log.auditable?.user_id) vars.user = agentName(log.auditable?.user_id, agents);
    const attrs: string[] = [];
    const values: string[] = [];
    if (changes.role?.length) {
      attrs.push("role");
      values.push(roleMapping[last(changes.role)]);
    }
    if (changes.availability?.length) {
      attrs.push("availability");
      values.push(availabilityMapping[last(changes.availability)]);
    }
    vars.attributes = attrs.join(", ");
    vars.values = values.join(", ");
  }
  if ((type === "inboxmember" || type === "teammember") && log.audited_changes) {
    if (changes.user_id) vars.user = agentName(changes.user_id, agents);
    if (changes.team_id) vars.team_id = changes.team_id;
    if (changes.inbox_id) vars.inbox_id = changes.inbox_id;
  }

  let key = `${type}:${action}`;
  if (type === "accountuser" && action === "update") {
    if (log.auditable === null) key += ":deleted";
    else key += log.user_id === log.auditable.user_id ? ":self" : ":other";
  }
  const msgKey = translationKeys[key];
  return msgKey ? t(msgKey, vars) : "";
}

const PER_PAGE_FALLBACK = 25;

export default function AuditLogsSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = acct(accountId);
  const [logs, setLogs] = useState<AuditLog[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [page, setPage] = useState(1);
  const [total, setTotal] = useState(0);
  const [perPage, setPerPage] = useState(PER_PAGE_FALLBACK);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    api<Agent[] | { payload: Agent[] }>(`${base}/agents`)
      .then(d => setAgents(Array.isArray(d) ? d : d.payload))
      .catch(() => setAgents([]));
  }, [base]);

  useEffect(() => {
    setLoading(true);
    setError("");
    api<Page>(`${base}/audit_logs?page=${page}`)
      .then(d => {
        setLogs(d.audit_logs);
        setTotal(d.total_entries);
        setPerPage(d.per_page || PER_PAGE_FALLBACK);
      })
      .catch(e => setError(e instanceof Error ? e.message : t("AUDIT_LOGS.API.ERROR_MESSAGE")))
      .finally(() => setLoading(false));
  }, [base, page]);

  const pages = Math.max(1, Math.ceil(total / perPage));

  return <section className="flex-1 overflow-auto p-8 text-n-slate-12">
    <header className="mb-6">
      <h1 className="text-xl font-medium">{t("AUDIT_LOGS.HEADER")}</h1>
      <p className="mt-2 text-sm text-n-slate-11">{t("AUDIT_LOGS.LIST.DESC")}</p>
    </header>
    {error && <p role="alert" className="mb-4 text-n-ruby-11">{error}</p>}
    {loading ? <p>{t("AUDIT_LOGS.LOADING")}</p> : logs.length === 0 ? <p className="text-sm text-n-slate-11">{t("AUDIT_LOGS.LIST.404")}</p> : <>
      <div className="overflow-x-auto">
        <table className="w-full text-left text-sm">
          <thead>
            <tr className="border-b border-n-weak text-n-slate-11">
              <th className="p-3 font-medium">{t("AUDIT_LOGS.LIST.TABLE_HEADER.ACTIVITY")}</th>
              <th className="whitespace-nowrap p-3 font-medium">{t("AUDIT_LOGS.LIST.TABLE_HEADER.TIME")}</th>
              <th className="whitespace-nowrap p-3 font-medium">{t("AUDIT_LOGS.LIST.TABLE_HEADER.IP_ADDRESS")}</th>
            </tr>
          </thead>
          <tbody>
            {logs.map(log => <tr key={log.id} className="border-b border-n-weak">
              <td className="p-3">{activity(log, agents)}</td>
              <td className="whitespace-nowrap p-3 text-n-slate-11">{new Date(log.created_at).toLocaleString()}</td>
              <td className="whitespace-nowrap p-3 text-n-slate-11">{log.remote_address ?? ""}</td>
            </tr>)}
          </tbody>
        </table>
      </div>
      <nav className="mt-4 flex items-center justify-end gap-3 text-sm" aria-label="Pagination">
        <button type="button" className="rounded-lg border border-n-weak px-3 py-1 disabled:opacity-50" disabled={page <= 1} onClick={() => setPage(p => p - 1)}>&lsaquo;</button>
        <span className="text-n-slate-11">{page} / {pages}</span>
        <button type="button" className="rounded-lg border border-n-weak px-3 py-1 disabled:opacity-50" disabled={page >= pages} onClick={() => setPage(p => p + 1)}>&rsaquo;</button>
      </nav>
    </>}
  </section>;
}
