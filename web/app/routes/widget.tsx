// Public website chat widget (customer side). Loaded in an iframe at /app/widget?website_token=...
// Talks to /api/v1/widget/* with the visitor token from POST /config; live replies over /cable.
import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { Send } from "lucide-react";
import { clock } from "~/lib/format";

type WidgetMessage = { id: number; content: string | null; message_type: number; created_at: number; echo_id?: string; sender?: { name?: string } };
type Config = {
  website_channel_config: { auth_token: string; website_name: string; welcome_title: string | null; welcome_tagline: string | null; widget_color: string | null; reply_time: string };
  contact: { pubsub_token: string };
};

const REPLY: Record<string, string> = {
  in_a_few_minutes: "Usually replies in a few minutes",
  in_a_few_hours: "Usually replies in a few hours",
  in_a_day: "Usually replies in a day",
};

export default function Widget() {
  const [params] = useSearchParams();
  const websiteToken = params.get("website_token") ?? "";
  const storeKey = `pulsar.widget.${websiteToken}`;
  const [cfg, setCfg] = useState<Config | null>(null);
  const [messages, setMessages] = useState<WidgetMessage[]>([]);
  const [text, setText] = useState("");
  const [error, setError] = useState("");
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

  // config: reuse the stored visitor token, start fresh if it was rejected
  useEffect(() => {
    if (!websiteToken) return setError("This chat is not set up");
    let live = true;
    const start = async () => {
      const saved = localStorage.getItem(storeKey) ?? "";
      let c: Config;
      try {
        c = await call("config", saved, { method: "POST" });
      } catch (e: any) {
        if (e.status !== 401) throw e;
        localStorage.removeItem(storeKey);
        c = await call("config", "", { method: "POST" });
      }
      localStorage.setItem(storeKey, c.website_channel_config.auth_token);
      const m = await call("messages", c.website_channel_config.auth_token);
      if (!live) return;
      setCfg(c);
      setMessages(m.payload ?? []);
    };
    start().catch((e) => live && setError(e.message));
    return () => {
      live = false;
    };
  }, [websiteToken]);

  // live agent replies
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
          setMessages((prev) => (prev.some((m) => m.id === e.data.id || (e.data.echo_id && m.echo_id === e.data.echo_id)) ? prev : [...prev, e.data]));
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
  }, [messages.length]);

  async function send(e: React.FormEvent) {
    e.preventDefault();
    const content = text.trim();
    if (!content || !cfg) return;
    setText("");
    setError("");
    const echo_id = crypto.randomUUID();
    try {
      const m: WidgetMessage = await call("messages", cfg.website_channel_config.auth_token, {
        method: "POST",
        body: JSON.stringify({ message: { content, echo_id, timestamp: new Date().toString(), referer_url: document.referrer } }),
      });
      setMessages((prev) => (prev.some((x) => x.id === m.id) ? prev : [...prev, { ...m, echo_id }]));
    } catch (err: any) {
      setText(content);
      setError(err.message);
    }
  }

  const ch = cfg?.website_channel_config;
  const color = ch?.widget_color || "#1f6fd1";

  return (
    <div className="flex h-dvh flex-col bg-surface">
      <header className="shrink-0 px-5 pb-4 pt-5 text-white" style={{ background: color }}>
        <p className="text-[15px] font-medium">{ch?.welcome_title || ch?.website_name || "Chat with us"}</p>
        <p className="mt-1 text-[13px] text-white/85">{ch?.welcome_tagline || (ch ? REPLY[ch.reply_time] : "")}</p>
      </header>
      <ol className="min-h-0 flex-1 space-y-2 overflow-y-auto bg-bg px-4 py-4">
        {messages.length === 0 && cfg && <li className="pt-6 text-center text-[13px] text-ink-3">Ask us anything, we read every message</li>}
        {messages.map((m) => {
          const mine = m.message_type === 0;
          return (
            <li key={m.id} className={`flex ${mine ? "justify-end" : "justify-start"}`}>
              <div
                className={`max-w-[80%] rounded-xl px-3 py-2 text-[14px] leading-relaxed ${mine ? "text-white" : "border border-line bg-surface text-ink"}`}
                style={mine ? { background: color } : undefined}
              >
                <p className="whitespace-pre-wrap break-words">{m.content}</p>
                <p className={`num mt-0.5 font-mono text-[10px] ${mine ? "text-white/70" : "text-ink-3"}`}>
                  {!mine && m.sender?.name ? `${m.sender.name}, ` : ""}
                  {clock(m.created_at)}
                </p>
              </div>
            </li>
          );
        })}
        <div ref={end} />
      </ol>
      {error && (
        <p role="alert" className="px-4 pt-2 text-[13px] text-bad">
          {error}
        </p>
      )}
      <form onSubmit={send} className="flex shrink-0 items-center gap-2 border-t border-line p-3">
        <input
          aria-label="Message"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Type your message"
          disabled={!cfg}
          className="h-10 min-w-0 flex-1 rounded-lg border border-line-strong bg-surface px-3 text-[14px] outline-none transition-colors duration-150 placeholder:text-ink-3 focus:border-accent"
        />
        <button
          type="submit"
          aria-label="Send message"
          disabled={!cfg || !text.trim()}
          className="grid size-10 shrink-0 place-items-center rounded-lg text-white transition-opacity duration-150 disabled:opacity-40"
          style={{ background: color }}
        >
          <Send size={16} strokeWidth={1.5} />
        </button>
      </form>
    </div>
  );
}
