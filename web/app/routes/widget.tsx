// Public website chat widget (customer side). Loaded in an iframe at /app/widget?website_token=...
// Multi-view Chatwoot parity: Home, Unread Drawer, PreChat, Messages (CSAT + Dyte + Cable), Campaigns, and Inline Help Center.
import {
  ArrowLeft,
  BookOpen,
  ChevronRight,
  Home,
  Mail,
  MessageSquare,
  Search,
  Send,
  Smile,
  Video,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { clock, initials } from "~/lib/format";
import { renderSafeMarkdown } from "~/lib/markdown";

type WidgetMessage = {
  id: number;
  content: string | null;
  message_type: number;
  created_at: number;
  echo_id?: string;
  sender?: { name?: string };
  content_type?: string;
  content_attributes?: {
    submitted_values?: { csat_survey_response?: { rating?: number } };
    type?: string;
    data?: { meeting_id?: string };
    external_error?: string;
  };
};

type AgentMember = {
  id: number;
  name: string;
  avatar_url?: string;
  availability_status: string;
};

type PreChatField = {
  name: string;
  label: string;
  placeholder?: string;
  required?: boolean;
  type?: string;
  enabled?: boolean;
};

type PortalSummary = { slug: string; name: string; default_locale?: string };
type PortalArticle = { id: number; title: string; slug: string; description?: string | null; content?: string | null };
type CampaignSummary = { id: number; title?: string; message?: string; trigger_rules?: { url?: string; time_on_page?: number } };

type Config = {
  website_channel_config: {
    auth_token: string;
    website_name: string;
    welcome_title: string | null;
    welcome_tagline: string | null;
    widget_color: string | null;
    reply_time: string;
    disable_branding?: boolean;
    pre_chat_form_enabled?: boolean;
    pre_chat_form_options?: {
      pre_chat_message?: string;
      pre_chat_fields?: PreChatField[];
    };
    portal?: PortalSummary | null;
  };
  contact: { pubsub_token: string; name?: string; email?: string };
};

const CSAT_RATINGS = [
  { value: 1, emoji: "😞", label: "Poor" },
  { value: 2, emoji: "😑", label: "Fair" },
  { value: 3, emoji: "😐", label: "Average" },
  { value: 4, emoji: "😀", label: "Good" },
  { value: 5, emoji: "😍", label: "Excellent" },
];

const QUICK_EMOJIS = ["😀", "😊", "🙏", "👍", "❤️", "🎉", "🔥", "💬", "👋", "😢", "😮", "✅"];

const REPLY: Record<string, string> = {
  in_a_few_minutes: "Typically replies in a few minutes",
  in_a_few_hours: "Typically replies in a few hours",
  in_a_day: "Typically replies in a day",
};

function Csat({ message, rate }: { message: WidgetMessage; rate: (id: number, rating: number) => Promise<void> }) {
  const picked = message.content_attributes?.submitted_values?.csat_survey_response?.rating;
  const [busy, setBusy] = useState(false);
  if (picked) return <p className="mt-2 text-xs text-n-slate-11">Thank you for submitting the rating</p>;
  return (
    <div className="mt-2" role="group" aria-label="Rate your conversation">
      <div className="flex gap-1">
        {CSAT_RATINGS.map((r) => (
          <button
            key={r.value}
            type="button"
            disabled={busy}
            aria-label={r.label}
            title={r.label}
            className="rounded-md p-1 text-2xl leading-none transition hover:scale-110 disabled:opacity-50"
            onClick={async () => {
              setBusy(true);
              await rate(message.id, r.value).finally(() => setBusy(false));
            }}
          >
            {r.emoji}
          </button>
        ))}
      </div>
    </div>
  );
}

/** campaignHelper.js isPatternMatchingWithURL. Native URLPattern where available, else `*` wildcards. */
export function urlMatches(pattern: string, url: string) {
  let p = pattern;
  if (p.endsWith("/")) p = p.slice(0, -1) + "*\\?*\\#*";
  const loc = new URL(url);
  if (loc.pathname.endsWith("/")) loc.pathname = loc.pathname.slice(0, -1);
  const UP = (globalThis as { URLPattern?: new (p: string) => { test: (u: string) => boolean } }).URLPattern;
  try {
    if (UP) return new UP(p).test(loc.toString());
  } catch {
    return false;
  }
  const re = new RegExp(
    "^" + p.replace(/\\([?#])/g, "$1").replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*") + "$"
  );
  return re.test(loc.toString());
}

export default function Widget() {
  const [params] = useSearchParams();
  const websiteToken = params.get("website_token") ?? "";
  const storeKey = `pulsar.widget.${websiteToken}`;

  const [cfg, setCfg] = useState<Config | null>(null);
  const [members, setMembers] = useState<AgentMember[]>([]);
  const [messages, setMessages] = useState<WidgetMessage[]>([]);
  const initialQueryView = params.get("view");
  const [view, setView] = useState<"home" | "prechat" | "messages" | "unread" | "article">(() =>
    initialQueryView === "unread-messages" ? "unread" : "home",
  );
  const [proactiveCampaign, setProactiveCampaign] = useState<CampaignSummary | null>(null);
  const [articles, setArticles] = useState<PortalArticle[]>([]);
  const [articleQuery, setArticleQuery] = useState("");
  const [activeArticle, setActiveArticle] = useState<PortalArticle | null>(null);
  const [dyteSession, setDyteSession] = useState<{
    messageId: number;
    authToken?: string;
    roomUrl?: string;
    error?: string;
  } | null>(null);
  const [dyteBusy, setDyteBusy] = useState(false);
  const [text, setText] = useState("");
  const [showEmoji, setShowEmoji] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  // Pre-chat form state
  const [preName, setPreName] = useState("");
  const [preEmail, setPreEmail] = useState("");
  const [preMessage, setPreMessage] = useState("");
  const [preBusy, setPreBusy] = useState(false);

  const end = useRef<HTMLDivElement>(null);

  const call = (path: string, token: string, init: RequestInit = {}) =>
    fetch(`/api/v1/widget/${path}${path.includes("?") ? "&" : "?"}website_token=${encodeURIComponent(websiteToken)}`, {
      ...init,
      headers: { "content-type": "application/json", ...(token ? { "X-Auth-Token": token } : {}) },
    }).then(async (r) => {
      const body = await r.json().catch(() => ({}));
      if (!r.ok) throw Object.assign(new Error(body.error ?? `Request failed (${r.status})`), { status: r.status });
      return body;
    });

  useEffect(() => {
    if (!websiteToken) return setError("This chat is not set up");
    let live = true;
    const start = async () => {
      const saved = localStorage.getItem(storeKey) ?? "";
      let c: Config;
      try {
        c = await call("config", saved, { method: "POST" });
      } catch (e: unknown) {
        if ((e as { status?: number })?.status !== 401) throw e;
        localStorage.removeItem(storeKey);
        c = await call("config", "", { method: "POST" });
      }
      localStorage.setItem(storeKey, c.website_channel_config.auth_token);
      const [m, mem] = await Promise.all([
        call("messages", c.website_channel_config.auth_token),
        call("inbox_members", "").catch(() => ({ payload: [] })),
      ]);
      if (!live) return;
      const msgs = (m.payload ?? []) as WidgetMessage[];
      setCfg(c);
      setMessages(msgs);
      setMembers(Array.isArray(mem?.payload) ? mem.payload : []);
      if (initialQueryView === "unread-messages") {
        setView("unread");
      } else if (msgs.length > 0) {
        setView("messages");
      }
      const portal = c.website_channel_config.portal;
      if (portal?.slug) {
        const loc = portal.default_locale || "en";
        fetch(`/hc/${encodeURIComponent(portal.slug)}/${encodeURIComponent(loc)}/articles.json`)
          .then((r) => (r.ok ? r.json() : null))
          .then((body) => {
            if (live && Array.isArray(body?.payload)) {
              setArticles(body.payload as PortalArticle[]);
            }
          })
          .catch(() => {});
      }
    };
    start().catch((e) => live && setError(e instanceof Error ? e.message : "Failed to initialize widget"));
    return () => {
      live = false;
    };
  }, [websiteToken, initialQueryView]);

  useEffect(() => {
    if (!cfg) return;
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const identifier = JSON.stringify({ channel: "RoomChannel", pubsub_token: cfg.contact.pubsub_token });
    let ws: WebSocket;
    let closed = false;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const open = () => {
      ws = new WebSocket(`${proto}://${location.host}/cable`, ["actioncable-v1-json", "actioncable-unsupported"]);
      ws.onopen = () => ws.send(JSON.stringify({ command: "subscribe", identifier }));
      ws.onmessage = (ev) => {
        const msg = JSON.parse(ev.data);
        const e = msg.message;
        if (e?.event === "message.created" && !e.data.private && e.data.message_type !== 2) {
          setMessages((prev) =>
            prev.some((m) => m.id === e.data.id || (e.data.echo_id && m.echo_id === e.data.echo_id))
              ? prev
              : [...prev, e.data]
          );
        }
      };
      ws.onclose = () => {
        if (!closed) retry = setTimeout(open, 2000);
      };
    };
    open();
    return () => {
      closed = true;
      clearTimeout(retry);
      ws?.close();
    };
  }, [cfg]);

  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [messages.length, view]);

  useEffect(() => {
    if (!cfg || messages.length > 0) return;
    const token = cfg.website_channel_config.auth_token;
    const page = document.referrer || location.href;
    const timers: ReturnType<typeof setTimeout>[] = [];
    let live = true;
    call("campaigns", token)
      .then((list: CampaignSummary[]) => {
        for (const c of list) {
          if (!live || !c.trigger_rules?.url || !urlMatches(c.trigger_rules.url, page)) continue;
          setProactiveCampaign((prev) => prev ?? c);
          timers.push(
            setTimeout(async () => {
              await triggerCampaign(c.id, token, page);
              const m = await call("messages", token);
              if (live) {
                setMessages(m.payload ?? []);
                setView("messages");
              }
            }, (c.trigger_rules.time_on_page ?? 0) * 1000)
          );
        }
      })
      .catch(() => {});
    return () => {
      live = false;
      timers.forEach(clearTimeout);
    };
  }, [cfg, messages.length > 0]);

  async function rate(id: number, rating: number) {
    if (!cfg) return;
    const submitted_values = { csat_survey_response: { rating, feedback_message: "" } };
    await call(`messages/${id}`, cfg.website_channel_config.auth_token, {
      method: "PATCH",
      body: JSON.stringify({ message: { submitted_values } }),
    });
    setMessages((prev) =>
      prev.map((x) => (x.id === id ? { ...x, content_attributes: { ...x.content_attributes, submitted_values } } : x))
    );
  }

  async function sendPreChat(e: React.FormEvent) {
    e.preventDefault();
    if (!cfg || !preMessage.trim()) return;
    setPreBusy(true);
    setError("");
    try {
      await call("conversations", cfg.website_channel_config.auth_token, {
        method: "POST",
        body: JSON.stringify({
          contact: { name: preName.trim() || undefined, email: preEmail.trim() || undefined },
          message: { content: preMessage.trim(), timestamp: new Date().toString(), referer_url: document.referrer },
        }),
      });
      const m = await call("messages", cfg.website_channel_config.auth_token);
      setMessages(m.payload ?? []);
      setPreMessage("");
      setView("messages");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not start conversation");
    } finally {
      setPreBusy(false);
    }
  }

  async function send(e: React.FormEvent) {
    e.preventDefault();
    const content = text.trim();
    if (!content || !cfg) return;
    setText("");
    setShowEmoji(false);
    setError("");
    const echo_id = crypto.randomUUID();
    try {
      const m: WidgetMessage = await call("messages", cfg.website_channel_config.auth_token, {
        method: "POST",
        body: JSON.stringify({
          message: { content, echo_id, timestamp: new Date().toString(), referer_url: document.referrer },
        }),
      });
      setMessages((prev) => (prev.some((x) => x.id === m.id) ? prev : [...prev, { ...m, echo_id }]));
    } catch (err) {
      setText(content);
      setError(err instanceof Error ? err.message : "Failed to send");
    }
  }

  async function triggerCampaign(campaignId: number, tok: string, pageUrl: string) {
    await call("events", tok, {
      method: "POST",
      body: JSON.stringify({
        name: "campaign.triggered",
        event_info: { campaign_id: campaignId, referer: pageUrl, initiated_at: { timestamp: new Date().toString() } },
      }),
    });
  }

  async function joinDyteMeeting(messageId: number) {
    if (!cfg || dyteBusy) return;
    setDyteBusy(true);
    try {
      const res = await call("integrations/dyte/add_participant_to_meeting", cfg.website_channel_config.auth_token, {
        method: "POST",
        body: JSON.stringify({ message_id: messageId }),
      });
      if (res?.error) {
        setDyteSession({ messageId, error: String(res.error) });
        return;
      }
      const authToken = res?.authResponse?.authToken ?? res?.data?.token ?? res?.token ?? res?.auth_token;
      const roomUrl =
        res?.room_url ?? (authToken ? `https://app.dyte.io/v2/meeting?authToken=${encodeURIComponent(authToken)}` : undefined);
      setDyteSession({ messageId, authToken, roomUrl });
      if (roomUrl) window.open(roomUrl, "_blank", "noopener,noreferrer");
    } catch (err) {
      setDyteSession({ messageId, error: err instanceof Error ? err.message : "Failed to join call" });
    } finally {
      setDyteBusy(false);
    }
  }

  async function requestTranscript() {
    if (!cfg) return;
    setNotice("");
    try {
      await call("conversations/transcript", cfg.website_channel_config.auth_token, { method: "POST" });
      setNotice("Transcript sent to your email.");
      setTimeout(() => setNotice(""), 2500);
    } catch {
      setNotice("Set your email in profile to receive transcripts.");
      setTimeout(() => setNotice(""), 2500);
    }
  }

  const ch = cfg?.website_channel_config;
  const color = ch?.widget_color || "#2781F6";
  const onlineMembers = members.filter((m) => m.availability_status === "online");
  const displayMembers = (onlineMembers.length > 0 ? onlineMembers : members).slice(0, 4);
  const replySubtext = ch ? REPLY[ch.reply_time] : "Typically replies in a few minutes";

  function handleStartClick() {
    if (ch?.pre_chat_form_enabled && messages.length === 0) {
      setView("prechat");
    } else {
      setView("messages");
    }
  }

  return (
    <div className="flex h-dvh flex-col bg-n-background text-n-slate-12">
      {/* Header */}
      {view === "home" ? (
        <header
          className="shrink-0 px-6 pb-10 pt-7 text-white"
          style={{
            background: `linear-gradient(155deg, ${color} 0%, ${color}e6 100%)`,
          }}
        >
          <div className="mb-4 flex items-center gap-2">
            <img src="/app/brand-assets/logo_thumbnail.svg" alt="Pulsar" className="size-7 rounded-lg bg-white p-0.5 shadow-sm" />
            <span className="text-sm font-medium text-white/90">{ch?.website_name || "Support"}</span>
          </div>
          <h1 className="font-inter-display text-2xl font-semibold leading-snug">
            {ch?.welcome_title || "Hi there!"}
          </h1>
          <p className="mt-1.5 text-sm leading-relaxed text-white/85">
            {ch?.welcome_tagline || "We make it simple to connect with us. Ask us anything, or share your feedback."}
          </p>
        </header>
      ) : (
        <header
          className="flex h-14 shrink-0 items-center justify-between px-4 text-white shadow-xs"
          style={{ background: color }}
        >
          <div className="flex min-w-0 items-center gap-2.5">
            <button
              type="button"
              onClick={() => {
                if (view === "article") setActiveArticle(null);
                setView("home");
              }}
              aria-label="Back to home"
              className="grid size-8 shrink-0 place-items-center rounded-lg hover:bg-white/15"
            >
              <ArrowLeft className="size-4" />
            </button>
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold leading-tight">
                {view === "article" && activeArticle ? activeArticle.title : ch?.website_name || "Chat with us"}
              </p>
              <p className="flex items-center gap-1.5 text-[11px] text-white/85">
                <span className="size-1.5 rounded-full bg-emerald-300" />
                {view === "unread" ? "Unread messages" : view === "article" ? ch?.portal?.name || "Help Center" : replySubtext}
              </p>
            </div>
          </div>
          {view === "messages" && messages.length > 0 && (
            <button
              type="button"
              onClick={() => void requestTranscript()}
              title="Email transcript"
              aria-label="Email transcript"
              className="grid size-8 shrink-0 place-items-center rounded-lg hover:bg-white/15"
            >
              <Mail className="size-4" />
            </button>
          )}
        </header>
      )}

      {/* Body View */}
      {view === "home" && (
        <div className="relative z-10 -mt-5 flex flex-1 flex-col gap-3 overflow-y-auto px-4 pb-4">
          <div className="flex flex-col gap-3 rounded-xl border border-n-weak bg-white p-5 shadow-md dark:bg-n-solid-2">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2 text-sm font-semibold text-n-slate-12">
                  <span className={`size-2 rounded-full ${onlineMembers.length > 0 ? "bg-n-teal-9" : "bg-n-amber-9"}`} />
                  {onlineMembers.length > 0 ? "We are online" : "We are away at the moment"}
                </div>
                <p className="mt-0.5 text-xs text-n-slate-11">{replySubtext}</p>
              </div>
              {displayMembers.length > 0 && (
                <div className="flex -space-x-2">
                  {displayMembers.map((m) => (
                    <span
                      key={m.id}
                      title={m.name}
                      className="grid size-8 place-items-center rounded-full border-2 border-white bg-n-blue-3 text-xs font-semibold text-n-blue-11 dark:border-n-solid-2"
                    >
                      {initials(m.name)}
                    </span>
                  ))}
                </div>
              )}
            </div>

            <button
              type="button"
              onClick={handleStartClick}
              disabled={!cfg}
              className="mt-1 inline-flex items-center justify-between rounded-lg border border-n-weak px-3.5 py-2.5 text-sm font-semibold transition hover:bg-n-alpha-1 disabled:opacity-50"
              style={{ color }}
            >
              <span>{messages.length > 0 ? "Continue conversation" : "Start conversation"}</span>
              <ChevronRight className="size-4" />
            </button>
          </div>

          {proactiveCampaign && messages.length === 0 && (
            <button
              type="button"
              onClick={async () => {
                if (!cfg) return;
                await triggerCampaign(proactiveCampaign.id, cfg.website_channel_config.auth_token, document.referrer || location.href).catch(() => {});
                const m = await call("messages", cfg.website_channel_config.auth_token);
                setMessages(m.payload ?? []);
                setView("messages");
              }}
              className="rounded-xl border border-n-weak bg-white p-4 text-left shadow-xs transition hover:bg-n-solid-2 dark:bg-n-solid-2"
            >
              <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
                {proactiveCampaign.title || "Proactive message"}
              </div>
              <p className="text-sm text-n-slate-12">{proactiveCampaign.message || "Chat with our team"}</p>
            </button>
          )}

          {messages.length > 0 && (
            <button
              type="button"
              onClick={() => setView("messages")}
              className="rounded-xl border border-n-weak bg-white p-4 text-left shadow-xs transition hover:bg-n-solid-2 dark:bg-n-solid-2"
            >
              <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
                Recent conversation
              </div>
              <p className="truncate text-sm text-n-slate-12">
                {messages[messages.length - 1]?.content || "Open conversation"}
              </p>
            </button>
          )}

          {ch?.portal && articles.length > 0 && (
            <div className="rounded-xl border border-n-weak bg-white p-4 shadow-xs dark:bg-n-solid-2">
              <div className="mb-2.5 flex items-center gap-1.5 text-xs font-semibold text-n-slate-12">
                <BookOpen className="size-3.5 text-n-brand" />
                <span>{ch.portal.name}</span>
              </div>
              <div className="relative mb-2.5">
                <Search className="pointer-events-none absolute left-2.5 top-2.5 size-3.5 text-n-slate-10" />
                <input
                  type="search"
                  value={articleQuery}
                  onChange={(e) => setArticleQuery(e.target.value)}
                  placeholder="Search Help Center articles..."
                  className="h-8 w-full rounded-lg border border-n-weak bg-n-background pl-8 pr-2.5 text-xs focus:border-n-brand focus:outline-none"
                />
              </div>
              <ul className="divide-y divide-n-weak">
                {articles
                  .filter((a) => !articleQuery.trim() || a.title.toLowerCase().includes(articleQuery.toLowerCase()))
                  .slice(0, 5)
                  .map((a) => (
                    <li key={a.id}>
                      <button
                        type="button"
                        onClick={() => {
                          setActiveArticle(a);
                          setView("article");
                        }}
                        className="flex w-full items-center justify-between gap-2 py-2 text-left text-xs text-n-slate-12 hover:text-n-brand"
                      >
                        <span className="truncate">{a.title}</span>
                        <ChevronRight className="size-3.5 shrink-0 text-n-slate-10" />
                      </button>
                    </li>
                  ))}
              </ul>
            </div>
          )}
        </div>
      )}

      {view === "unread" && (
        <div className="flex flex-1 flex-col justify-between overflow-y-auto p-4">
          <div className="space-y-2.5">
            {messages
              .filter((m) => m.message_type !== 0)
              .slice(-5)
              .map((m) => (
                <button
                  key={m.id}
                  type="button"
                  onClick={() => setView("messages")}
                  className="w-full rounded-xl border border-n-weak bg-n-solid-1 p-3.5 text-left shadow-xs transition hover:border-n-brand"
                >
                  <div className="mb-1 flex items-center justify-between text-[11px] text-n-slate-10">
                    <span>{m.sender?.name || ch?.website_name || "Support"}</span>
                    <span>{clock(m.created_at)}</span>
                  </div>
                  <p className="line-clamp-2 text-sm text-n-slate-12">{m.content || "Attachment"}</p>
                </button>
              ))}
            {messages.filter((m) => m.message_type !== 0).length === 0 && (
              <p className="py-6 text-center text-xs text-n-slate-10">No unread messages.</p>
            )}
          </div>
          <button
            type="button"
            onClick={() => setView("messages")}
            className="mt-4 flex h-10 w-full items-center justify-center rounded-lg text-sm font-medium text-white"
            style={{ background: color }}
          >
            Open messages
          </button>
        </div>
      )}

      {view === "article" && activeArticle && (
        <article className="flex-1 overflow-y-auto bg-n-background p-5">
          <h2 className="font-inter-display text-lg font-semibold text-n-slate-12">{activeArticle.title}</h2>
          {activeArticle.description && (
            <p className="mt-1 text-xs text-n-slate-11">{activeArticle.description}</p>
          )}
          <div
            className="mt-4 text-sm leading-relaxed text-n-slate-12"
            dangerouslySetInnerHTML={{
              __html: renderSafeMarkdown(activeArticle.content || activeArticle.description || ""),
            }}
          />
          {ch?.portal?.slug && (
            <div className="mt-6 border-t border-n-weak pt-3">
              <a
                href={`/hc/${encodeURIComponent(ch.portal.slug)}/articles/${encodeURIComponent(activeArticle.slug)}`}
                target="_blank"
                rel="noopener noreferrer"
                className="text-xs font-medium text-n-brand hover:underline"
              >
                Read in Help Center →
              </a>
            </div>
          )}
        </article>
      )}

      {view === "prechat" && (
        <form onSubmit={sendPreChat} className="flex flex-1 flex-col gap-3.5 overflow-y-auto p-5">
          <div className="rounded-xl border border-n-weak bg-n-solid-1 p-4 space-y-3.5 shadow-xs">
            <p className="text-xs leading-relaxed text-n-slate-11">
              {ch?.pre_chat_form_options?.pre_chat_message || "Please share your details before starting a conversation."}
            </p>
            <label className="block space-y-1 text-xs font-medium text-n-slate-12">
              Name
              <input
                value={preName}
                onChange={(e) => setPreName(e.target.value)}
                placeholder="Your name"
                className="h-9 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm focus:border-n-brand focus:outline-none"
              />
            </label>
            <label className="block space-y-1 text-xs font-medium text-n-slate-12">
              Email Address
              <input
                required
                type="email"
                value={preEmail}
                onChange={(e) => setPreEmail(e.target.value)}
                placeholder="you@company.com"
                className="h-9 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm focus:border-n-brand focus:outline-none"
              />
            </label>
            <label className="block space-y-1 text-xs font-medium text-n-slate-12">
              Message
              <textarea
                required
                rows={3}
                value={preMessage}
                onChange={(e) => setPreMessage(e.target.value)}
                placeholder="How can we help?"
                className="w-full rounded-lg border border-n-weak bg-n-background p-3 text-sm focus:border-n-brand focus:outline-none"
              />
            </label>
            <button
              type="submit"
              disabled={preBusy || !preMessage.trim()}
              className="flex h-10 w-full items-center justify-center rounded-lg text-sm font-medium text-white transition disabled:opacity-50"
              style={{ background: color }}
            >
              {preBusy ? "Starting..." : "Start Conversation"}
            </button>
          </div>
        </form>
      )}

      {view === "messages" && (
        <>
          {notice && (
            <div className="bg-n-teal-3 px-4 py-1.5 text-center text-xs text-n-teal-11">
              {notice}
            </div>
          )}
          <ol className="m-0 min-h-0 flex-1 list-none space-y-2.5 overflow-y-auto px-4 py-4">
            {messages.length === 0 && cfg && (
              <li className="pt-8 text-center text-xs text-n-slate-10">
                Ask us anything, we read every message
              </li>
            )}
            {messages.map((m) => {
              const mine = m.message_type === 0;
              return (
                <li key={m.id} className={`flex ${mine ? "justify-end" : "justify-start"}`}>
                  <div
                    className={`max-w-[82%] rounded-2xl px-3.5 py-2 text-sm leading-relaxed shadow-2xs ${
                      mine
                        ? "rounded-br-sm text-white"
                        : "rounded-bl-sm border border-n-weak bg-n-solid-2 text-n-slate-12"
                    }`}
                    style={mine ? { background: color } : undefined}
                  >
                    <p className="whitespace-pre-wrap break-words">{m.content}</p>
                    {m.content_type === "input_csat" && <Csat message={m} rate={rate} />}
                    {m.content_type === "integrations" && m.content_attributes?.type === "dyte" && (
                      <div className="mt-2 space-y-1.5">
                        <button
                          type="button"
                          disabled={dyteBusy}
                          onClick={() => void joinDyteMeeting(m.id)}
                          className="inline-flex items-center gap-1.5 rounded-lg bg-n-brand px-3 py-1.5 text-xs font-medium text-white hover:bg-n-blue-10 disabled:opacity-50"
                        >
                          <Video className="size-3.5" />
                          <span>{dyteBusy ? "Connecting..." : "Join video call"}</span>
                        </button>
                        {dyteSession?.messageId === m.id && dyteSession.error && (
                          <p className="text-xs text-n-ruby-11">{dyteSession.error}</p>
                        )}
                        {dyteSession?.messageId === m.id && dyteSession.roomUrl && (
                          <a
                            href={dyteSession.roomUrl}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="block text-xs underline text-n-brand"
                          >
                            Open meeting room
                          </a>
                        )}
                      </div>
                    )}
                    <p
                      className={`mt-1 font-mono text-[10px] ${
                        mine ? "text-white/75" : "text-n-slate-10"
                      }`}
                    >
                      {!mine && m.sender?.name ? `${m.sender.name} • ` : ""}
                      {clock(m.created_at)}
                    </p>
                  </div>
                </li>
              );
            })}
            <div ref={end} />
          </ol>

          {error && (
            <p role="alert" className="px-4 py-1 text-xs text-n-ruby-11">
              {error}
            </p>
          )}

          <div className="relative shrink-0 border-t border-n-weak bg-n-solid-1 p-3">
            {showEmoji && (
              <div className="absolute bottom-full right-3 mb-2 grid grid-cols-6 gap-1 rounded-xl border border-n-weak bg-n-solid-1 p-2 shadow-xl">
                {QUICK_EMOJIS.map((em) => (
                  <button
                    key={em}
                    type="button"
                    onClick={() => {
                      setText((t) => t + em);
                      setShowEmoji(false);
                    }}
                    className="grid size-8 place-items-center rounded text-lg hover:bg-n-alpha-2"
                  >
                    {em}
                  </button>
                ))}
              </div>
            )}
            <form onSubmit={send} className="flex items-center gap-2 rounded-xl border border-n-weak bg-n-background px-2.5 py-1.5 focus-within:border-n-brand">
              <input
                aria-label="Message"
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder="Type your message..."
                disabled={!cfg}
                className="h-8 min-w-0 flex-1 bg-transparent text-sm text-n-slate-12 placeholder:text-n-slate-10 focus:outline-none"
              />
              <button
                type="button"
                onClick={() => setShowEmoji((v) => !v)}
                aria-label="Pick emoji"
                className="grid size-8 shrink-0 place-items-center rounded-lg text-n-slate-10 hover:bg-n-alpha-2 hover:text-n-slate-12"
              >
                <Smile className="size-4" />
              </button>
              <button
                type="submit"
                aria-label="Send message"
                disabled={!cfg || !text.trim()}
                className="grid size-8 shrink-0 place-items-center rounded-lg text-white transition disabled:opacity-40"
                style={{ background: color }}
              >
                <Send className="size-4" />
              </button>
            </form>
          </div>
        </>
      )}

      {/* Bottom Tab Navigation (Home | Messages) on Home/Messages + Branding footer */}
      {view !== "prechat" && (
        <footer className="shrink-0 border-t border-n-weak bg-n-solid-1">
          <div className="grid grid-cols-2">
            <button
              type="button"
              onClick={() => setView("home")}
              className={`flex flex-col items-center gap-0.5 py-2 text-[11px] font-medium transition ${
                view === "home" ? "text-n-slate-12" : "text-n-slate-10 hover:text-n-slate-12"
              }`}
              style={view === "home" ? { color } : undefined}
            >
              <Home className="size-4" />
              Home
            </button>
            <button
              type="button"
              onClick={() => setView("messages")}
              className={`flex flex-col items-center gap-0.5 py-2 text-[11px] font-medium transition ${
                view === "messages" ? "text-n-slate-12" : "text-n-slate-10 hover:text-n-slate-12"
              }`}
              style={view === "messages" ? { color } : undefined}
            >
              <MessageSquare className="size-4" />
              Messages
            </button>
          </div>
          {!ch?.disable_branding && (
            <div className="pb-2 text-center text-[10px] text-n-slate-10">
              Powered by Pulsar
            </div>
          )}
        </footer>
      )}
    </div>
  );
}
