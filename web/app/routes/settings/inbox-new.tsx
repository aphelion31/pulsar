import {
  Camera,
  Check,
  ChevronLeft,
  Code2,
  Copy,
  Globe,
  Mail,
  MessageCircle,
  MessageSquare,
  Phone,
  PhoneCall,
  PhoneForwarded,
  Search,
  Send,
  Smartphone,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useOutletContext, useParams } from "react-router";
import Avatar from "~/components/next/Avatar";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type Inbox = {
  id: number;
  name: string;
  channel_type: string;
  website_url?: string | null;
  webhook_url?: string | null;
  widget_color?: string | null;
  welcome_title?: string | null;
  welcome_tagline?: string | null;
  inbox_identifier?: string | null;
  hmac_token?: string | null;
  web_widget_script?: string | null;
  email?: string | null;
  forward_to_email?: string | null;
  forwarding_enabled?: boolean | null;
};

type Agent = {
  id: number;
  name: string;
  email: string;
  avatar_url?: string;
  thumbnail?: string;
  role?: string;
};

const COLOR_PRESETS = [
  "#009CE0",
  "#1f93ff",
  "#00C2A8",
  "#27AE60",
  "#F2994A",
  "#EB5757",
  "#8E44AD",
  "#34495E",
];

export default function InboxNewWizard() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { sub_page, inbox_id } = useParams();
  const location = useLocation();
  const navigate = useNavigate();

  const isFinish = location.pathname.endsWith("/finish");
  const isAgents = location.pathname.endsWith("/agents");
  const isSubPage = Boolean(sub_page);

  const activeStep = isFinish ? 3 : isAgents ? 2 : isSubPage ? 1 : 0;

  // Form states for Step 2
  const [inboxName, setInboxName] = useState("");
  const [websiteUrl, setWebsiteUrl] = useState("");
  const [widgetColor, setWidgetColor] = useState("#009CE0");
  const [welcomeTitle, setWelcomeTitle] = useState("");
  const [welcomeTagline, setWelcomeTagline] = useState("");
  const [greetingEnabled, setGreetingEnabled] = useState(false);
  const [greetingMessage, setGreetingMessage] = useState("");
  const [webhookUrl, setWebhookUrl] = useState("");
  const [emailAddress, setEmailAddress] = useState("");

  // Step 3 agents state
  const [allAgents, setAllAgents] = useState<Agent[]>([]);
  const [selectedAgentIds, setSelectedAgentIds] = useState<number[]>([]);
  const [agentSearch, setAgentSearch] = useState("");

  // Step 4 finish state
  const [currentInbox, setCurrentInbox] = useState<Inbox | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // General state
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const steps = [
    {
      key: "CHANNEL",
      title: t("INBOX_MGMT.CREATE_FLOW.CHANNEL.TITLE"),
      body: t("INBOX_MGMT.CREATE_FLOW.CHANNEL.BODY"),
    },
    {
      key: "INBOX",
      title: t("INBOX_MGMT.CREATE_FLOW.INBOX.TITLE"),
      body: t("INBOX_MGMT.CREATE_FLOW.INBOX.BODY"),
    },
    {
      key: "AGENT",
      title: t("INBOX_MGMT.CREATE_FLOW.AGENT.TITLE"),
      body: t("INBOX_MGMT.CREATE_FLOW.AGENT.BODY"),
    },
    {
      key: "FINISH",
      title: t("INBOX_MGMT.CREATE_FLOW.FINISH.TITLE"),
      body: t("INBOX_MGMT.CREATE_FLOW.FINISH.BODY"),
    },
  ];

  const channelCards = [
    {
      key: "website",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.WEBSITE.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.WEBSITE.DESCRIPTION"),
      icon: Globe,
      active: true,
    },
    {
      key: "facebook",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.FACEBOOK.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.FACEBOOK.DESCRIPTION"),
      icon: MessageCircle,
      active: false,
    },
    {
      key: "whatsapp",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.WHATSAPP.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.WHATSAPP.DESCRIPTION"),
      icon: Phone,
      active: false,
    },
    {
      key: "sms",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.SMS.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.SMS.DESCRIPTION"),
      icon: Smartphone,
      active: false,
    },
    {
      key: "email",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.EMAIL.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.EMAIL.DESCRIPTION"),
      icon: Mail,
      active: true,
    },
    {
      key: "api",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.API.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.API.DESCRIPTION"),
      icon: Code2,
      active: true,
    },
    {
      key: "telegram",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.TELEGRAM.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.TELEGRAM.DESCRIPTION"),
      icon: Send,
      active: false,
    },
    {
      key: "line",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.LINE.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.LINE.DESCRIPTION"),
      icon: MessageSquare,
      active: false,
    },
    {
      key: "instagram",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.INSTAGRAM.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.INSTAGRAM.DESCRIPTION"),
      icon: Camera,
      active: false,
    },
    {
      key: "voice",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.VOICE.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.VOICE.DESCRIPTION"),
      icon: PhoneCall,
      active: false,
      beta: true,
    },
    {
      key: "whatsapp_call",
      title: t("INBOX_MGMT.ADD.AUTH.CHANNEL.WHATSAPP_CALL.TITLE"),
      description: t("INBOX_MGMT.ADD.AUTH.CHANNEL.WHATSAPP_CALL.DESCRIPTION"),
      icon: PhoneForwarded,
      active: false,
      beta: true,
    },
  ];

  // Fetch agents on Step 3
  useEffect(() => {
    if (!isAgents || !inbox_id) return;
    let active = true;
    setError("");
    Promise.all([
      api<Agent[]>(`${acct(accountId)}/agents`),
      api<{ payload: Agent[] }>(`${acct(accountId)}/inbox_members/${inbox_id}`),
    ])
      .then(([agents, members]) => {
        if (!active) return;
        setAllAgents(agents);
        const memberIds = members.payload.map((m) => m.id);
        setSelectedAgentIds(memberIds.length > 0 ? memberIds : agents.slice(0, 1).map((a) => a.id));
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load agents");
      });
    return () => {
      active = false;
    };
  }, [accountId, inbox_id, isAgents]);

  // Fetch inbox details on Step 4
  useEffect(() => {
    if (!isFinish || !inbox_id) return;
    let active = true;
    setError("");
    api<Inbox>(`${acct(accountId)}/inboxes/${inbox_id}`)
      .then((data) => {
        if (active) setCurrentInbox(data);
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load inbox details");
      });
    return () => {
      active = false;
    };
  }, [accountId, inbox_id, isFinish]);

  async function copyToClipboard(key: string, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedKey(key);
      setTimeout(() => setCopiedKey(null), 2000);
    } catch {
      // ignore
    }
  }

  // Handle Step 2 submission
  async function handleCreateInbox(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      let payload: Record<string, unknown> = {};
      if (sub_page === "website") {
        if (!inboxName.trim() || !websiteUrl.trim()) {
          setLoading(false);
          return;
        }
        payload = {
          name: inboxName.trim(),
          greeting_enabled: greetingEnabled,
          greeting_message: greetingMessage,
          channel: {
            type: "web_widget",
            website_url: websiteUrl.trim(),
            widget_color: widgetColor,
            welcome_title: welcomeTitle.trim() || null,
            welcome_tagline: welcomeTagline.trim() || null,
          },
        };
      } else if (sub_page === "api") {
        if (!inboxName.trim()) {
          setLoading(false);
          return;
        }
        payload = {
          name: inboxName.trim(),
          channel: {
            type: "api",
            webhook_url: webhookUrl.trim() || null,
          },
        };
      } else if (sub_page === "email") {
        if (!inboxName.trim() || !emailAddress.trim()) {
          setLoading(false);
          return;
        }
        payload = {
          name: inboxName.trim(),
          channel: {
            type: "email",
            email: emailAddress.trim(),
          },
        };
      }

      const res = await api<Inbox>(`${acct(accountId)}/inboxes`, {
        method: "POST",
        json: payload,
      });

      navigate(`/accounts/${accountId}/settings/inboxes/new/${res.id}/agents`, { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : t("INBOX_MGMT.DETAILS.ERROR_CHANNEL_CREATION"));
    } finally {
      setLoading(false);
    }
  }

  // Handle Step 3 submission
  async function handleAddAgents(e: React.FormEvent) {
    e.preventDefault();
    if (selectedAgentIds.length === 0) {
      setError(t("INBOX_MGMT.ADD.AGENTS.VALIDATION_ERROR"));
      return;
    }
    setError("");
    setLoading(true);

    try {
      await api(`${acct(accountId)}/inbox_members`, {
        method: "POST",
        json: {
          inbox_id: Number(inbox_id),
          user_ids: selectedAgentIds,
        },
      });

      navigate(`/accounts/${accountId}/settings/inboxes/new/${inbox_id}/finish`, { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update inbox agents");
    } finally {
      setLoading(false);
    }
  }

  const selectedAgentObjects = allAgents.filter((a) => selectedAgentIds.includes(a.id));
  const filteredAgents = allAgents.filter(
    (a) =>
      a.name.toLowerCase().includes(agentSearch.toLowerCase()) ||
      a.email.toLowerCase().includes(agentSearch.toLowerCase())
  );

  return (
    <div className="flex h-full w-full flex-col overflow-y-auto bg-n-surface-1">
      {/* Top Header */}
      <div className="mx-auto flex h-16 w-full max-w-7xl items-center justify-between px-6">
        <h1 className="flex items-center text-xl font-medium text-n-slate-12">
          <Link
            to={`/accounts/${accountId}/settings/inboxes/list`}
            className="flex items-center gap-1 text-sm font-normal text-n-slate-11 hover:text-n-slate-12 mr-4"
          >
            <ChevronLeft className="size-4" />
            {t("GENERAL_SETTINGS.BACK")}
          </Link>
          <span>{t("INBOX_MGMT.HEADER")}</span>
        </h1>
      </div>

      {/* Main wizard container */}
      <div className="mx-auto mb-8 flex w-full max-w-7xl flex-1 flex-col px-6">
        <div className="grid min-h-[50dvh] flex-1 grid-cols-1 rounded-xl border border-n-weak bg-n-solid-1 lg:grid-cols-8 lg:divide-x lg:divide-n-weak">
          {/* Stepper (Left 2 cols) */}
          <div className="col-span-2 hidden h-fit flex-col gap-6 p-8 lg:flex">
            {steps.map((step, idx) => {
              const isActive = idx === activeStep;
              const isOver = idx < activeStep;
              return (
                <div key={step.key} className="relative flex items-start gap-4">
                  {idx < steps.length - 1 && (
                    <div
                      className={`absolute left-4 top-8 -bottom-4 w-0.5 ${
                        isOver ? "bg-n-brand" : "bg-n-slate-3"
                      }`}
                    />
                  )}
                  <div
                    className={`relative z-10 flex size-8 shrink-0 items-center justify-center rounded-2xl border-2 transition-all ${
                      isActive || isOver
                        ? "border-n-brand bg-n-brand text-white"
                        : "border-n-slate-3 bg-n-background text-n-slate-11"
                    }`}
                  >
                    {isOver ? (
                      <Check className="size-4" />
                    ) : (
                      <span className="text-xs font-bold">{idx + 1}</span>
                    )}
                  </div>
                  <div className="flex flex-col gap-1 pb-6 pt-0.5">
                    <h3
                      className={`text-sm font-medium leading-tight ${
                        isActive ? "text-n-blue-11" : "text-n-slate-12"
                      }`}
                    >
                      {step.title}
                    </h3>
                    <p className="m-0 text-sm text-n-slate-11">{step.body}</p>
                  </div>
                </div>
              );
            })}
          </div>

          {/* Wizard Content (Right 6 cols) */}
          <div className="col-span-6 flex min-h-0 flex-col overflow-y-auto p-6 sm:p-8">
            {error && (
              <div
                role="alert"
                className="mb-6 rounded-lg border border-n-ruby-8 bg-n-ruby-2 p-3 text-sm text-n-ruby-11"
              >
                {error}
              </div>
            )}

            {/* STEP 1: CHANNEL LIST */}
            {activeStep === 0 && (
              <div className="flex flex-col gap-6">
                <div>
                  <h2 className="text-xl font-medium text-n-slate-12">
                    {t("INBOX_MGMT.ADD.AUTH.TITLE")}
                  </h2>
                  <p className="mt-1 text-sm text-n-slate-11">
                    {t("INBOX_MGMT.ADD.AUTH.DESC")}
                  </p>
                </div>

                <div className="grid max-w-4xl grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
                  {channelCards.map((channel) => (
                    <button
                      key={channel.key}
                      type="button"
                      disabled={!channel.active}
                      onClick={() => {
                        if (channel.active) {
                          navigate(`/accounts/${accountId}/settings/inboxes/new/${channel.key}`);
                        }
                      }}
                      className={`group relative flex flex-col items-start justify-start gap-4 rounded-2xl border border-n-weak bg-n-solid-1 p-5 text-left transition-all ${
                        channel.active
                          ? "cursor-pointer hover:border-n-blue-9 hover:shadow-md"
                          : "cursor-not-allowed opacity-60"
                      }`}
                    >
                      <div className="flex size-10 items-center justify-center rounded-full bg-n-alpha-2">
                        <channel.icon className="size-5 text-n-slate-10 group-hover:text-n-slate-12" />
                      </div>
                      <div className="flex flex-col items-start gap-1">
                        <div className="flex items-center gap-2">
                          <h3 className="text-sm font-medium capitalize text-n-slate-12">
                            {channel.title}
                          </h3>
                          {channel.beta && (
                            <span className="rounded-md bg-n-blue-2 px-1.5 py-0.5 text-[10px] font-medium text-n-blue-11 outline outline-1 -outline-offset-1 outline-n-blue-4">
                              {t("GENERAL.BETA")}
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-n-slate-11">{channel.description}</p>
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* STEP 2: CHANNEL FORM (Website or API) */}
            {activeStep === 1 && (
              <div className="max-w-2xl">
                {sub_page === "website" ? (
                  <>
                    <div className="mb-6">
                      <h2 className="text-xl font-medium text-n-slate-12">
                        {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.TITLE")}
                      </h2>
                      <p className="mt-1 text-sm text-n-slate-11">
                        {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.DESC")}
                      </p>
                    </div>

                    <form onSubmit={handleCreateInbox} className="flex flex-col gap-4">
                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.WEBSITE_NAME.LABEL")}{" "}
                          <span className="text-n-ruby-9">*</span>
                        </span>
                        <input
                          required
                          type="text"
                          value={inboxName}
                          onChange={(e) => setInboxName(e.target.value)}
                          placeholder={t("INBOX_MGMT.ADD.WEBSITE_NAME.PLACEHOLDER")}
                          className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        />
                      </label>

                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_DOMAIN.LABEL")}{" "}
                          <span className="text-n-ruby-9">*</span>
                        </span>
                        <input
                          required
                          type="text"
                          value={websiteUrl}
                          onChange={(e) => setWebsiteUrl(e.target.value)}
                          placeholder={t(
                            "INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_DOMAIN.PLACEHOLDER"
                          )}
                          className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        />
                      </label>

                      <div className="flex flex-col gap-2 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.WIDGET_COLOR.LABEL")}
                        </span>
                        <div className="flex flex-wrap items-center gap-2">
                          {COLOR_PRESETS.map((color) => (
                            <button
                              key={color}
                              type="button"
                              onClick={() => setWidgetColor(color)}
                              style={{ backgroundColor: color }}
                              className={`size-7 rounded-full transition-transform ${
                                widgetColor.toLowerCase() === color.toLowerCase()
                                  ? "ring-2 ring-n-slate-12 ring-offset-2 scale-110"
                                  : "hover:scale-105"
                              }`}
                              aria-label={`Color ${color}`}
                            />
                          ))}
                          <div className="flex items-center gap-2 pl-2">
                            <input
                              type="color"
                              value={widgetColor}
                              onChange={(e) => setWidgetColor(e.target.value)}
                              className="size-8 cursor-pointer rounded border-0 bg-transparent p-0"
                            />
                            <input
                              type="text"
                              value={widgetColor}
                              onChange={(e) => setWidgetColor(e.target.value)}
                              className="h-8 w-24 rounded-lg border border-n-weak bg-n-background px-2 font-mono text-xs text-n-slate-12"
                            />
                          </div>
                        </div>
                      </div>

                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t(
                            "INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_WELCOME_TITLE.LABEL"
                          )}
                        </span>
                        <input
                          type="text"
                          value={welcomeTitle}
                          onChange={(e) => setWelcomeTitle(e.target.value)}
                          placeholder={t(
                            "INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_WELCOME_TITLE.PLACEHOLDER"
                          )}
                          className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        />
                      </label>

                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t(
                            "INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_WELCOME_TAGLINE.LABEL"
                          )}
                        </span>
                        <textarea
                          rows={2}
                          value={welcomeTagline}
                          onChange={(e) => setWelcomeTagline(e.target.value)}
                          placeholder={t(
                            "INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_WELCOME_TAGLINE.PLACEHOLDER"
                          )}
                          className="rounded-lg border border-n-weak bg-n-background p-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        />
                      </label>

                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_GREETING_TOGGLE.LABEL")}
                        </span>
                        <select
                          value={greetingEnabled ? "true" : "false"}
                          onChange={(e) => setGreetingEnabled(e.target.value === "true")}
                          className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        >
                          <option value="true">
                            {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_GREETING_TOGGLE.ENABLED")}
                          </option>
                          <option value="false">
                            {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_GREETING_TOGGLE.DISABLED")}
                          </option>
                        </select>
                        <span className="text-xs text-n-slate-11">
                          {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_GREETING_TOGGLE.HELP_TEXT")}
                        </span>
                      </label>

                      {greetingEnabled && (
                        <label className="flex flex-col gap-1.5 text-sm">
                          <span className="font-medium text-n-slate-12">
                            {t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_GREETING_MESSAGE.LABEL")}
                          </span>
                          <textarea
                            rows={3}
                            value={greetingMessage}
                            onChange={(e) => setGreetingMessage(e.target.value)}
                            placeholder={t(
                              "INBOX_MGMT.ADD.WEBSITE_CHANNEL.CHANNEL_GREETING_MESSAGE.PLACEHOLDER"
                            )}
                            className="rounded-lg border border-n-weak bg-n-background p-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                          />
                        </label>
                      )}

                      <div className="mt-4 flex justify-end gap-2">
                        <Button
                          solid
                          blue
                          type="submit"
                          isLoading={loading}
                          disabled={loading || !inboxName.trim() || !websiteUrl.trim()}
                          label={t("INBOX_MGMT.ADD.WEBSITE_CHANNEL.SUBMIT_BUTTON")}
                        />
                      </div>
                    </form>
                  </>
                ) : sub_page === "api" ? (
                  <>
                    <div className="mb-6">
                      <h2 className="text-xl font-medium text-n-slate-12">
                        {t("INBOX_MGMT.ADD.API_CHANNEL.TITLE")}
                      </h2>
                      <p className="mt-1 text-sm text-n-slate-11">
                        {t("INBOX_MGMT.ADD.API_CHANNEL.DESC")}
                      </p>
                    </div>

                    <form onSubmit={handleCreateInbox} className="flex flex-col gap-4">
                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.API_CHANNEL.CHANNEL_NAME.LABEL")}{" "}
                          <span className="text-n-ruby-9">*</span>
                        </span>
                        <input
                          required
                          type="text"
                          value={inboxName}
                          onChange={(e) => setInboxName(e.target.value)}
                          placeholder={t("INBOX_MGMT.ADD.API_CHANNEL.CHANNEL_NAME.PLACEHOLDER")}
                          className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        />
                      </label>

                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.API_CHANNEL.WEBHOOK_URL.LABEL")}
                        </span>
                        <input
                          type="url"
                          value={webhookUrl}
                          onChange={(e) => setWebhookUrl(e.target.value)}
                          placeholder={t("INBOX_MGMT.ADD.API_CHANNEL.WEBHOOK_URL.PLACEHOLDER")}
                          className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        />
                        <span className="text-xs text-n-slate-11">
                          {t("INBOX_MGMT.ADD.API_CHANNEL.WEBHOOK_URL.SUBTITLE")}
                        </span>
                      </label>

                      <div className="mt-4 flex justify-end gap-2">
                        <Button
                          solid
                          blue
                          type="submit"
                          isLoading={loading}
                          disabled={loading || !inboxName.trim()}
                          label={t("INBOX_MGMT.ADD.API_CHANNEL.SUBMIT_BUTTON")}
                        />
                      </div>
                    </form>
                  </>
                ) : sub_page === "email" ? (
                  <>
                    <div className="mb-6">
                      <h2 className="text-xl font-medium text-n-slate-12">
                        {t("INBOX_MGMT.ADD.EMAIL_CHANNEL.TITLE")}
                      </h2>
                      <p className="mt-1 text-sm text-n-slate-11">
                        {t("INBOX_MGMT.ADD.EMAIL_CHANNEL.DESC")}
                      </p>
                    </div>

                    <form onSubmit={handleCreateInbox} className="flex flex-col gap-4">
                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.EMAIL_CHANNEL.CHANNEL_NAME.LABEL")}{" "}
                          <span className="text-n-ruby-9">*</span>
                        </span>
                        <input
                          required
                          type="text"
                          value={inboxName}
                          onChange={(e) => setInboxName(e.target.value)}
                          placeholder={t(
                            "INBOX_MGMT.ADD.EMAIL_CHANNEL.CHANNEL_NAME.PLACEHOLDER"
                          )}
                          className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        />
                      </label>

                      <label className="flex flex-col gap-1.5 text-sm">
                        <span className="font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.EMAIL_CHANNEL.EMAIL.LABEL")}{" "}
                          <span className="text-n-ruby-9">*</span>
                        </span>
                        <input
                          required
                          type="email"
                          value={emailAddress}
                          onChange={(e) => setEmailAddress(e.target.value)}
                          placeholder={t(
                            "INBOX_MGMT.ADD.EMAIL_CHANNEL.EMAIL.PLACEHOLDER"
                          )}
                          className="h-10 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                        />
                        <span className="text-xs text-n-slate-11">
                          {t("INBOX_MGMT.ADD.EMAIL_CHANNEL.EMAIL.SUBTITLE")}
                        </span>
                      </label>

                      <div className="mt-4 flex justify-end gap-2">
                        <Button
                          solid
                          blue
                          type="submit"
                          isLoading={loading}
                          disabled={
                            loading || !inboxName.trim() || !emailAddress.trim()
                          }
                          label={t("INBOX_MGMT.ADD.EMAIL_CHANNEL.SUBMIT_BUTTON")}
                        />
                      </div>
                    </form>
                  </>
                ) : (
                  <div className="text-sm text-n-slate-11">
                    Channel configuration not supported in this wizard.
                  </div>
                )}
              </div>
            )}

            {/* STEP 3: ADD AGENTS */}
            {activeStep === 2 && (
              <div className="max-w-2xl">
                <div className="mb-6">
                  <h2 className="text-xl font-medium text-n-slate-12">
                    {t("INBOX_MGMT.ADD.AGENTS.TITLE")}
                  </h2>
                  <p
                    className="mt-1 text-sm text-n-slate-11"
                    dangerouslySetInnerHTML={{ __html: t("INBOX_MGMT.ADD.AGENTS.DESC") }}
                  />
                </div>

                <form onSubmit={handleAddAgents} className="flex flex-col gap-4">
                  {/* Selected agent tags */}
                  {selectedAgentObjects.length > 0 && (
                    <div className="flex flex-wrap gap-2 rounded-xl border border-n-weak bg-n-background p-3">
                      {selectedAgentObjects.map((agent) => (
                        <span
                          key={agent.id}
                          className="inline-flex items-center gap-1.5 rounded-lg border border-n-weak bg-n-solid-2 px-2.5 py-1 text-xs text-n-slate-12"
                        >
                          <Avatar name={agent.name} src={agent.thumbnail || agent.avatar_url} size={18} />
                          <span>{agent.name}</span>
                          <button
                            type="button"
                            aria-label={`Remove ${agent.name}`}
                            onClick={() =>
                              setSelectedAgentIds(selectedAgentIds.filter((id) => id !== agent.id))
                            }
                            className="text-n-slate-10 hover:text-n-ruby-11"
                          >
                            <X className="size-3.5" />
                          </button>
                        </span>
                      ))}
                    </div>
                  )}

                  {/* Search and list */}
                  <div className="flex flex-col gap-2 rounded-xl border border-n-weak bg-n-background p-3">
                    <div className="relative">
                      <Search className="absolute left-3 top-3 size-4 text-n-slate-10" />
                      <input
                        type="text"
                        value={agentSearch}
                        onChange={(e) => setAgentSearch(e.target.value)}
                        placeholder={t("INBOX_MGMT.ADD.AGENTS.PICK_AGENTS")}
                        className="h-10 w-full rounded-lg border border-n-weak bg-n-solid-1 pl-9 pr-3 text-sm text-n-slate-12 outline-none focus:border-n-brand"
                      />
                    </div>

                    <div className="max-h-64 divide-y divide-n-weak overflow-y-auto">
                      {filteredAgents.length === 0 ? (
                        <p className="p-4 text-center text-xs text-n-slate-10">No agents found</p>
                      ) : (
                        filteredAgents.map((agent) => {
                          const isSelected = selectedAgentIds.includes(agent.id);
                          return (
                            <label
                              key={agent.id}
                              className="flex cursor-pointer items-center justify-between p-2.5 hover:bg-n-alpha-2"
                            >
                              <div className="flex items-center gap-3">
                                <Avatar
                                  name={agent.name}
                                  src={agent.thumbnail || agent.avatar_url}
                                  size={28}
                                />
                                <div>
                                  <div className="text-sm font-medium text-n-slate-12">{agent.name}</div>
                                  <div className="text-xs text-n-slate-11">{agent.email}</div>
                                </div>
                              </div>
                              <input
                                type="checkbox"
                                checked={isSelected}
                                onChange={(e) => {
                                  if (e.target.checked) {
                                    setSelectedAgentIds([...selectedAgentIds, agent.id]);
                                  } else {
                                    setSelectedAgentIds(selectedAgentIds.filter((id) => id !== agent.id));
                                  }
                                }}
                                className="size-4 rounded border-n-weak text-n-brand focus:ring-0"
                              />
                            </label>
                          );
                        })
                      )}
                    </div>
                  </div>

                  <div className="mt-4 flex justify-end gap-2">
                    <Button
                      solid
                      blue
                      type="submit"
                      isLoading={loading}
                      disabled={loading || selectedAgentIds.length === 0}
                      label={t("INBOX_MGMT.AGENTS.BUTTON_TEXT")}
                    />
                  </div>
                </form>
              </div>
            )}

            {/* STEP 4: FINISH */}
            {activeStep === 3 && (
              <div className="mx-auto flex w-full max-w-2xl flex-col items-center py-8 text-center">
                <div className="mb-4 flex size-14 items-center justify-center rounded-full bg-n-teal-2 text-n-teal-11">
                  <Check className="size-8" />
                </div>
                <h2 className="text-2xl font-medium text-n-slate-12">
                  {t("INBOX_MGMT.FINISH.TITLE")}
                </h2>
                <p className="mt-2 max-w-lg text-sm text-n-slate-11">
                  {currentInbox?.channel_type === "Channel::WebWidget"
                    ? t("INBOX_MGMT.FINISH.WEBSITE_SUCCESS")
                    : currentInbox?.channel_type === "Channel::Email"
                    ? currentInbox.forwarding_enabled
                      ? t("INBOX_MGMT.ADD.EMAIL_CHANNEL.FINISH_MESSAGE")
                      : t("INBOX_MGMT.ADD.EMAIL_CHANNEL.FINISH_MESSAGE_NO_FORWARDING")
                    : t("INBOX_MGMT.FINISH.MESSAGE")}
                </p>

                {/* Website script box */}
                {currentInbox?.web_widget_script && (
                  <div className="mt-6 w-full text-left">
                    <div className="flex items-center justify-between pb-1 text-xs text-n-slate-11">
                      <span>Widget Script</span>
                      <button
                        type="button"
                        onClick={() => copyToClipboard("script", currentInbox.web_widget_script || "")}
                        className="inline-flex items-center gap-1 font-medium text-n-blue-11 hover:underline"
                      >
                        {copiedKey === "script" ? (
                          <>
                            <Check className="size-3" />
                            {t("COMPONENTS.CODE.COPY_SUCCESSFUL")}
                          </>
                        ) : (
                          <>
                            <Copy className="size-3" />
                            {t("COMPONENTS.CODE.BUTTON_TEXT")}
                          </>
                        )}
                      </button>
                    </div>
                    <pre className="overflow-x-auto rounded-xl border border-n-weak bg-n-solid-2 p-4 font-mono text-xs text-n-slate-12">
                      <code>{currentInbox.web_widget_script}</code>
                    </pre>
                  </div>
                )}

                {/* API channel info */}
                {currentInbox?.channel_type === "Channel::Api" && (
                  <div className="mt-6 flex w-full flex-col gap-4 text-left">
                    {currentInbox.inbox_identifier && (
                      <div>
                        <div className="flex items-center justify-between pb-1 text-xs text-n-slate-11">
                          <span className="font-medium text-n-slate-12">
                            {t("INBOX_MGMT.SETTINGS_POPUP.INBOX_IDENTIFIER")}
                          </span>
                          <button
                            type="button"
                            onClick={() =>
                              copyToClipboard("identifier", currentInbox.inbox_identifier || "")
                            }
                            className="inline-flex items-center gap-1 font-medium text-n-blue-11 hover:underline"
                          >
                            {copiedKey === "identifier" ? (
                              <>
                                <Check className="size-3" />
                                {t("COMPONENTS.CODE.COPY_SUCCESSFUL")}
                              </>
                            ) : (
                              <>
                                <Copy className="size-3" />
                                {t("COMPONENTS.CODE.BUTTON_TEXT")}
                              </>
                            )}
                          </button>
                        </div>
                        <input
                          readOnly
                          value={currentInbox.inbox_identifier}
                          className="h-10 w-full rounded-lg border border-n-weak bg-n-solid-2 px-3 font-mono text-xs text-n-slate-12"
                        />
                        <p className="mt-1 text-xs text-n-slate-11">
                          {t("INBOX_MGMT.SETTINGS_POPUP.INBOX_IDENTIFIER_SUB_TEXT")}
                        </p>
                      </div>
                    )}

                    {currentInbox.webhook_url && (
                      <div>
                        <span className="text-xs font-medium text-n-slate-12">
                          {t("INBOX_MGMT.ADD.API_CHANNEL.WEBHOOK_URL.LABEL")}
                        </span>
                        <input
                          readOnly
                          value={currentInbox.webhook_url}
                          className="mt-1 h-10 w-full rounded-lg border border-n-weak bg-n-solid-2 px-3 text-xs text-n-slate-12"
                        />
                      </div>
                    )}
                  </div>
                )}

                {/* Email channel info */}
                {currentInbox?.channel_type === "Channel::Email" && (
                  <div className="mt-6 flex w-full flex-col gap-4 text-left">
                    {currentInbox.forwarding_enabled && currentInbox.forward_to_email && (
                      <div>
                        <div className="flex items-center justify-between pb-1 text-xs text-n-slate-11">
                          <span className="font-medium text-n-slate-12">
                            {t("INBOX_MGMT.ADD.EMAIL_CHANNEL.FORWARDING_ADDRESS_LABEL")}
                          </span>
                          <button
                            type="button"
                            onClick={() =>
                              copyToClipboard("forward_to", currentInbox.forward_to_email || "")
                            }
                            className="inline-flex items-center gap-1 font-medium text-n-blue-11 hover:underline"
                          >
                            {copiedKey === "forward_to" ? (
                              <>
                                <Check className="size-3" />
                                {t("COMPONENTS.CODE.COPY_SUCCESSFUL")}
                              </>
                            ) : (
                              <>
                                <Copy className="size-3" />
                                {t("COMPONENTS.CODE.BUTTON_TEXT")}
                              </>
                            )}
                          </button>
                        </div>
                        <input
                          readOnly
                          value={currentInbox.forward_to_email}
                          className="h-10 w-full rounded-lg border border-n-weak bg-n-solid-2 px-3 font-mono text-xs text-n-slate-12"
                        />
                      </div>
                    )}

                    <p className="text-center text-sm text-n-slate-11">
                      <Link
                        to={`/accounts/${accountId}/settings/inboxes/${inbox_id}/configuration`}
                        className="text-n-blue-11 underline hover:text-n-blue-12"
                      >
                        {t("INBOX_MGMT.ADD.EMAIL_CHANNEL.CONFIGURE_SMTP_IMAP_LINK")}
                      </Link>
                      {t("INBOX_MGMT.ADD.EMAIL_CHANNEL.CONFIGURE_SMTP_IMAP_TEXT")}
                    </p>
                  </div>
                )}

                <div className="mt-8 flex gap-3">
                  <Link to={`/accounts/${accountId}/settings/inboxes/${inbox_id}`}>
                    <Button outline slate label={t("INBOX_MGMT.FINISH.MORE_SETTINGS")} />
                  </Link>
                  <Link to={`/accounts/${accountId}/inbox/${inbox_id}`}>
                    <Button solid teal label={t("INBOX_MGMT.FINISH.BUTTON_TEXT")} />
                  </Link>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
