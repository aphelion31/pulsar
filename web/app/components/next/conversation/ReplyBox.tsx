// Chatwoot-style ReplyBox: Reply / Private Note tabs, Markdown toolbar, "/" Canned, "@" Mentions, Emoji Picker, KB Article insert, Voice Recorder, Attachments, Captain AI.
import {
  BookOpen,
  Maximize2,
  Mic,
  Minimize2,
  Paperclip,
  Send,
  Slash,
  Smile,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import Avatar from "~/components/next/Avatar";
import { RichTextEditor } from "~/components/next/RichTextEditor";
import { acct, api, type Agent, type Canned, type Message } from "~/lib/api";
import { bytes } from "~/lib/format";

const EMOJI_GROUPS: Array<{ name: string; emojis: string[] }> = [
  { name: "Smileys", emojis: ["😀", "😄", "😊", "😉", "😍", "🥳", "🤔", "😅", "🙏", "👍", "👏", "❤️", "🎉", "🔥", "✨", "✅"] },
  { name: "Support", emojis: ["👋", "💬", "📩", "📞", "📎", "🔎", "🛠️", "⚡", "🚀", "📦", "🧾", "💳", "⏰", "📅", "🔒", "💡"] },
];

type PortalSummary = { slug: string; name: string };
type ArticleSummary = { id: number; title: string; slug: string; description?: string | null };

export function ReplyBox({
  base,
  accountId,
  conversationDisplayId,
  inboxId,
  inReplyTo,
  onClearReplyTo,
  onSent,
  onCaptainSummary,
}: {
  base: string;
  accountId: number;
  conversationDisplayId: number;
  inboxId?: number;
  inReplyTo?: Message | null;
  onClearReplyTo?: () => void;
  onSent: (m: Message) => void;
  onCaptainSummary?: (summary: string) => void;
}) {
  const [text, setText] = useState("");
  const [note, setNote] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [files, setFiles] = useState<File[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // Captain AI
  const [aiOpen, setAiOpen] = useState(false);
  const [aiBusy, setAiBusy] = useState(false);

  // Popups: emoji, canned responses, agent mentions, help center articles
  const [showEmoji, setShowEmoji] = useState(false);
  const [emojiSearch, setEmojiSearch] = useState("");
  const [cannedList, setCannedList] = useState<Canned[]>([]);
  const [cannedOpen, setCannedOpen] = useState(false);
  const [cannedFilter, setCannedFilter] = useState("");
  const [agents, setAgents] = useState<Agent[]>([]);
  const [mentionOpen, setMentionOpen] = useState(false);
  const [mentionFilter, setMentionFilter] = useState("");
  const [articleOpen, setArticleOpen] = useState(false);
  const [articles, setArticles] = useState<Array<ArticleSummary & { portalSlug: string }>>([]);
  const [articleQuery, setArticleQuery] = useState("");

  // Voice recording
  const [recording, setRecording] = useState(false);
  const [recordSecs, setRecordSecs] = useState(0);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let live = true;
    api<Canned[]>(`${acct(accountId)}/canned_responses`)
      .then((res) => { if (live && Array.isArray(res)) setCannedList(res); })
      .catch(() => {});
    api<Agent[]>(`${acct(accountId)}/agents`)
      .then((res) => { if (live && Array.isArray(res)) setAgents(res); })
      .catch(() => {});
    return () => { live = false; };
  }, [accountId]);

  useEffect(() => {
    if (!recording) return;
    const t = setInterval(() => setRecordSecs((s) => s + 1), 1000);
    return () => clearInterval(t);
  }, [recording]);

  async function toggleAudioRecorder() {
    if (recording) {
      recorderRef.current?.stop();
      setRecording(false);
      return;
    }
    setError("");
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const rec = new MediaRecorder(stream);
      chunksRef.current = [];
      rec.ondataavailable = (ev) => {
        if (ev.data.size > 0) chunksRef.current.push(ev.data);
      };
      rec.onstop = () => {
        stream.getTracks().forEach((t) => t.stop());
        const blob = new Blob(chunksRef.current, { type: "audio/webm" });
        const f = new File([blob], `voice-${Date.now()}.webm`, { type: "audio/webm" });
        setFiles((prev) => [...prev, f]);
        setRecordSecs(0);
      };
      recorderRef.current = rec;
      rec.start();
      setRecording(true);
    } catch {
      setError("Microphone access unavailable");
    }
  }

  async function openArticles() {
    setArticleOpen((v) => !v);
    setShowEmoji(false);
    setAiOpen(false);
    if (articles.length > 0) return;
    try {
      const pRes = await api<PortalSummary[] | { payload: PortalSummary[] }>(`${acct(accountId)}/portals`);
      const portals = Array.isArray(pRes) ? pRes : pRes.payload ?? [];
      if (!portals.length) return;
      const slug = portals[0].slug;
      const aRes = await api<{ payload: ArticleSummary[] }>(`${acct(accountId)}/portals/${slug}/articles`);
      setArticles((aRes.payload ?? []).map((a) => ({ ...a, portalSlug: slug })));
    } catch {
      // ignore if no portal
    }
  }

  function insertText(snippet: string) {
    const el = textareaRef.current;
    if (!el) {
      setText((prev) => prev + snippet);
      return;
    }
    const pos = el.selectionStart ?? text.length;
    const next = text.slice(0, pos) + snippet + text.slice(pos);
    setText(next);
    setTimeout(() => {
      el.focus();
      el.setSelectionRange(pos + snippet.length, pos + snippet.length);
    }, 0);
  }

  function handleTextChange(val: string) {
    setText(val);
    const pos = textareaRef.current?.selectionStart ?? val.length;
    const before = val.slice(0, pos);

    const cannedMatch = before.match(/(?:^|\s)\/([a-zA-Z0-9_-]*)$/);
    if (cannedMatch) {
      setCannedFilter(cannedMatch[1].toLowerCase());
      setCannedOpen(true);
      setMentionOpen(false);
      return;
    }
    setCannedOpen(false);

    if (note) {
      const mentionMatch = before.match(/(?:^|\s)@([a-zA-Z0-9_.-]*)$/);
      if (mentionMatch) {
        setMentionFilter(mentionMatch[1].toLowerCase());
        setMentionOpen(true);
        return;
      }
    }
    setMentionOpen(false);
  }

  function pickCanned(c: Canned) {
    const pos = textareaRef.current?.selectionStart ?? text.length;
    const before = text.slice(0, pos).replace(/(?:^|\s)\/[a-zA-Z0-9_-]*$/, (m) => (m.startsWith(" ") ? " " : ""));
    const after = text.slice(pos);
    setText(before + c.content + after);
    setCannedOpen(false);
    setTimeout(() => textareaRef.current?.focus(), 0);
  }

  function pickMention(a: Agent) {
    const pos = textareaRef.current?.selectionStart ?? text.length;
    const before = text.slice(0, pos).replace(/(?:^|\s)@[a-zA-Z0-9_.-]*$/, (m) => (m.startsWith(" ") ? " " : ""));
    const after = text.slice(pos);
    const label = `@${a.available_name || a.name} `;
    setText(before + label + after);
    setMentionOpen(false);
    setTimeout(() => textareaRef.current?.focus(), 0);
  }

  async function runCaptainTask(kind: "reply_suggestion" | "rewrite" | "summarize") {
    setAiBusy(true);
    setAiOpen(false);
    setError("");
    try {
      if (kind === "rewrite") {
        if (!text.trim()) throw new Error("Type a draft first to rewrite");
        const res = await api<{ message: string }>(`${acct(accountId)}/captain/tasks/rewrite`, {
          method: "POST",
          json: { content: text, operation: "improve" },
        });
        if (res.message) setText(res.message);
      } else if (kind === "reply_suggestion") {
        const res = await api<{ message: string }>(`${acct(accountId)}/captain/tasks/reply_suggestion`, {
          method: "POST",
          json: { conversation_display_id: conversationDisplayId, inbox_id: inboxId },
        });
        if (res.message) setText(res.message);
      } else {
        const res = await api<{ message: string }>(`${acct(accountId)}/captain/tasks/summarize`, {
          method: "POST",
          json: { conversation_display_id: conversationDisplayId },
        });
        onCaptainSummary?.(res.message || "");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Captain AI failed");
    } finally {
      setAiBusy(false);
    }
  }

  async function send() {
    const content = text;
    if (!content.trim() && files.length === 0) return;
    setBusy(true);
    setError("");
    setText("");
    try {
      const echo_id = crypto.randomUUID();
      let m: Message;
      if (files.length) {
        const fd = new FormData();
        if (content.trim()) fd.append("content", content);
        fd.append("private", String(note));
        fd.append("echo_id", echo_id);
        if (inReplyTo?.id) fd.append("content_attributes[in_reply_to]", String(inReplyTo.id));
        files.forEach((f) => fd.append("attachments[]", f));
        m = await api<Message>(`${base}/messages`, { method: "POST", body: fd });
      } else {
        m = await api<Message>(`${base}/messages`, {
          method: "POST",
          json: {
            content,
            private: note,
            echo_id,
            ...(inReplyTo?.id ? { content_attributes: { in_reply_to: inReplyTo.id } } : {}),
          },
        });
      }
      onSent(m);
      setFiles([]);
      onClearReplyTo?.();
    } catch (e) {
      setText(content);
      setError(e instanceof Error ? e.message : "Could not send");
    } finally {
      setBusy(false);
    }
  }

  const filteredCanned = useMemo(
    () =>
      cannedList
        .filter(
          (c) =>
            !cannedFilter ||
            c.short_code.toLowerCase().includes(cannedFilter) ||
            c.content.toLowerCase().includes(cannedFilter)
        )
        .slice(0, 8),
    [cannedList, cannedFilter]
  );

  const filteredAgents = useMemo(
    () =>
      agents
        .filter(
          (a) =>
            !mentionFilter ||
            (a.available_name || a.name).toLowerCase().includes(mentionFilter) ||
            a.email.toLowerCase().includes(mentionFilter)
        )
        .slice(0, 8),
    [agents, mentionFilter]
  );

  const filteredArticles = useMemo(() => {
    const q = articleQuery.trim().toLowerCase();
    return articles
      .filter((a) => !q || a.title.toLowerCase().includes(q))
      .slice(0, 8);
  }, [articles, articleQuery]);

  return (
    <div className="shrink-0 bg-n-background px-3 pb-3 pt-1">
      <div
        className={`relative mx-auto max-w-4xl rounded-xl border transition-colors ${
          note
            ? "border-n-amber-6/50 bg-n-solid-amber"
            : "border-n-weak bg-n-solid-1 shadow-xs"
        }`}
      >
        {/* Popovers anchored above composer */}
        {cannedOpen && filteredCanned.length > 0 && (
          <div className="absolute bottom-full left-3 mb-2 z-40 w-80 max-h-56 overflow-y-auto rounded-xl border border-n-weak bg-n-solid-1 p-1.5 shadow-xl">
            <div className="px-2 py-1 text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
              Canned Responses
            </div>
            {filteredCanned.map((c) => (
              <button
                key={c.id}
                type="button"
                onClick={() => pickCanned(c)}
                className="flex w-full flex-col rounded-lg px-2.5 py-1.5 text-left hover:bg-n-alpha-2"
              >
                <span className="font-mono text-xs font-medium text-n-blue-11">/{c.short_code}</span>
                <span className="truncate text-xs text-n-slate-11">{c.content}</span>
              </button>
            ))}
          </div>
        )}

        {mentionOpen && filteredAgents.length > 0 && (
          <div className="absolute bottom-full left-3 mb-2 z-40 w-72 max-h-56 overflow-y-auto rounded-xl border border-n-weak bg-n-solid-1 p-1.5 shadow-xl">
            <div className="px-2 py-1 text-[11px] font-medium uppercase tracking-wider text-n-slate-10">
              Mention Agent
            </div>
            {filteredAgents.map((a) => (
              <button
                key={a.id}
                type="button"
                onClick={() => pickMention(a)}
                className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs hover:bg-n-alpha-2"
              >
                <Avatar size={20} name={a.available_name || a.name} />
                <span className="truncate font-medium text-n-slate-12">{a.available_name || a.name}</span>
                <span className="ml-auto truncate text-n-slate-10">{a.role}</span>
              </button>
            ))}
          </div>
        )}

        {showEmoji && (
          <div className="absolute bottom-full left-3 mb-2 z-40 w-72 rounded-xl border border-n-weak bg-n-solid-1 p-2.5 shadow-xl">
            <input
              value={emojiSearch}
              onChange={(e) => setEmojiSearch(e.target.value)}
              placeholder="Pick an emoji..."
              className="mb-2 h-8 w-full rounded-lg border border-n-weak bg-n-alpha-black2 px-2.5 text-xs text-n-slate-12 focus:outline-none focus:border-n-brand"
            />
            {EMOJI_GROUPS.map((g) => (
              <div key={g.name} className="mb-2 last:mb-0">
                <div className="mb-1 text-[10px] font-medium uppercase text-n-slate-10">{g.name}</div>
                <div className="grid grid-cols-8 gap-1">
                  {g.emojis.map((em) => (
                    <button
                      key={em}
                      type="button"
                      onClick={() => {
                        insertText(em);
                        setShowEmoji(false);
                      }}
                      className="grid size-7 place-items-center rounded text-base hover:bg-n-alpha-2"
                    >
                      {em}
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}

        {articleOpen && (
          <div className="absolute bottom-full left-20 mb-2 z-40 w-80 rounded-xl border border-n-weak bg-n-solid-1 p-2 shadow-xl">
            <div className="mb-1.5 flex items-center justify-between px-1">
              <span className="text-xs font-medium text-n-slate-12">Insert Help Center Article</span>
              <button type="button" onClick={() => setArticleOpen(false)} className="text-n-slate-10 hover:text-n-slate-12">
                <X className="size-3.5" />
              </button>
            </div>
            <input
              value={articleQuery}
              onChange={(e) => setArticleQuery(e.target.value)}
              placeholder="Search articles..."
              className="mb-2 h-8 w-full rounded-lg border border-n-weak bg-n-alpha-black2 px-2.5 text-xs text-n-slate-12 focus:outline-none"
            />
            <div className="max-h-44 space-y-1 overflow-y-auto">
              {filteredArticles.length === 0 ? (
                <p className="py-3 text-center text-xs text-n-slate-10">No articles found</p>
              ) : (
                filteredArticles.map((a) => (
                  <button
                    key={a.id}
                    type="button"
                    onClick={() => {
                      insertText(`[${a.title}](/hc/${a.portalSlug}/articles/${a.slug})`);
                      setArticleOpen(false);
                    }}
                    className="block w-full truncate rounded-lg px-2 py-1.5 text-left text-xs text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    {a.title}
                  </button>
                ))
              )}
            </div>
          </div>
        )}

        {/* Top header strip: Reply / Private Note tabs + Captain + Expand */}
        <div className="flex items-center justify-between border-b border-n-weak px-3">
          <div role="tablist" className="flex items-center gap-4">
            <button
              type="button"
              role="tab"
              aria-selected={!note}
              onClick={() => setNote(false)}
              className={`-mb-px border-b-2 py-2.5 text-xs font-medium transition-colors ${
                !note
                  ? "border-n-brand text-n-slate-12"
                  : "border-transparent text-n-slate-10 hover:text-n-slate-12"
              }`}
            >
              Reply
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={note}
              onClick={() => setNote(true)}
              className={`-mb-px border-b-2 py-2.5 text-xs font-medium transition-colors ${
                note
                  ? "border-n-amber-9 text-n-amber-11"
                  : "border-transparent text-n-slate-10 hover:text-n-slate-12"
              }`}
            >
              Private Note
            </button>
          </div>

          <div className="relative flex items-center gap-1">
            <div className="relative">
              <button
                type="button"
                disabled={aiBusy}
                onClick={() => setAiOpen((v) => !v)}
                aria-label="Captain AI"
                className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-n-iris-11 hover:bg-n-alpha-2 disabled:opacity-50"
              >
                <Sparkles className="size-3.5" />
                <span className="hidden sm:inline">{aiBusy ? "Thinking..." : "Copilot"}</span>
              </button>
              {aiOpen && (
                <div className="absolute right-0 bottom-full mb-1 z-40 w-44 rounded-xl border border-n-weak bg-n-solid-1 p-1 shadow-xl">
                  <button
                    type="button"
                    onClick={() => void runCaptainTask("reply_suggestion")}
                    className="block w-full rounded-lg px-2.5 py-1.5 text-left text-xs text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    Suggest reply
                  </button>
                  <button
                    type="button"
                    disabled={!text.trim()}
                    onClick={() => void runCaptainTask("rewrite")}
                    className="block w-full rounded-lg px-2.5 py-1.5 text-left text-xs text-n-slate-12 hover:bg-n-alpha-2 disabled:opacity-40"
                  >
                    Improve draft
                  </button>
                  <button
                    type="button"
                    onClick={() => void runCaptainTask("summarize")}
                    className="block w-full rounded-lg px-2.5 py-1.5 text-left text-xs text-n-slate-12 hover:bg-n-alpha-2"
                  >
                    Summarize thread
                  </button>
                </div>
              )}
            </div>
            <button
              type="button"
              onClick={() => setExpanded((v) => !v)}
              aria-label={expanded ? "Collapse editor" : "Expand editor"}
              className="grid size-7 place-items-center rounded-md text-n-slate-10 hover:bg-n-alpha-2 hover:text-n-slate-12"
            >
              {expanded ? <Minimize2 className="size-3.5" /> : <Maximize2 className="size-3.5" />}
            </button>
          </div>
        </div>

        {/* Quoted Reply-To Preview */}
        {inReplyTo && (
          <div className="mx-3 mt-2.5 flex items-center justify-between gap-2 rounded-lg border border-n-weak bg-n-alpha-1 px-2.5 py-1.5 text-xs">
            <div className="min-w-0">
              <span className="font-medium text-n-slate-11">
                Replying to {inReplyTo.sender?.name || "message"}:{" "}
              </span>
              <span className="truncate text-n-slate-10">{inReplyTo.content || "Attachment"}</span>
            </div>
            <button
              type="button"
              onClick={onClearReplyTo}
              aria-label="Cancel reply"
              className="shrink-0 text-n-slate-10 hover:text-n-slate-12"
            >
              <X className="size-3.5" />
            </button>
          </div>
        )}

        {/* Dual-mode Rich WYSIWYG + Markdown Editor */}
        <div className="px-3 pt-2">
          <RichTextEditor
            value={text}
            onChange={handleTextChange}
            compact
            allowSplit
            ariaLabel={note ? "Private note" : "Reply"}
            rows={expanded ? 9 : 3}
            minHeightClass={expanded ? "min-h-[168px]" : "min-h-[76px]"}
            textareaRef={textareaRef}
            placeholder={
              note
                ? "Write a private note... Type @ to mention teammates or / for canned responses"
                : "Shift + Enter for new line. Start with '/' to select a Canned Response."
            }
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void send();
              } else if (e.key === "Escape") {
                setCannedOpen(false);
                setMentionOpen(false);
                setShowEmoji(false);
                setArticleOpen(false);
              }
            }}
          />

          {files.length > 0 && (
            <ul className="mb-2 flex flex-wrap gap-2">
              {files.map((f, i) => (
                <li
                  key={`${f.name}-${i}`}
                  className="flex items-center gap-2 rounded-lg border border-n-weak bg-n-solid-2 px-2.5 py-1 text-xs text-n-slate-12"
                >
                  <Paperclip className="size-3 text-n-slate-10" />
                  <span className="max-w-44 truncate">{f.name}</span>
                  <span className="font-mono text-n-slate-10">{bytes(f.size)}</span>
                  <button
                    type="button"
                    aria-label={`Remove ${f.name}`}
                    className="text-n-slate-10 hover:text-n-slate-12"
                    onClick={() => setFiles(files.filter((_, j) => j !== i))}
                  >
                    <X className="size-3.5" />
                  </button>
                </li>
              ))}
            </ul>
          )}

          {error && (
            <p role="alert" className="mb-2 text-xs text-n-ruby-11">
              {error}
            </p>
          )}
        </div>

        {/* Bottom Actions Bar */}
        <div className="flex items-center justify-between px-3 pb-2.5 pt-1">
          <div className="flex items-center gap-1">
            <input
              ref={fileInput}
              type="file"
              multiple
              hidden
              onChange={(e) => setFiles([...files, ...Array.from(e.target.files ?? [])])}
            />
            <button
              type="button"
              onClick={() => { setShowEmoji((v) => !v); setArticleOpen(false); }}
              aria-label="Emoji picker"
              className="grid size-7 place-items-center rounded-lg text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
            >
              <Smile className="size-4" />
            </button>
            <button
              type="button"
              onClick={() => fileInput.current?.click()}
              aria-label="Attach files"
              className="grid size-7 place-items-center rounded-lg text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
            >
              <Paperclip className="size-4" />
            </button>
            <button
              type="button"
              onClick={() => {
                setCannedOpen((v) => !v);
                setCannedFilter("");
              }}
              title="Canned responses (/)"
              aria-label="Insert canned response"
              className="grid size-7 place-items-center rounded-lg text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
            >
              <Slash className="size-4" />
            </button>
            <button
              type="button"
              onClick={() => void openArticles()}
              title="Insert Help Center article"
              aria-label="Insert article"
              className="grid size-7 place-items-center rounded-lg text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
            >
              <BookOpen className="size-4" />
            </button>
            <button
              type="button"
              onClick={() => void toggleAudioRecorder()}
              title={recording ? "Stop recording" : "Record voice note"}
              aria-label={recording ? "Stop recording" : "Record voice note"}
              className={`inline-flex h-7 items-center gap-1 rounded-lg px-1.5 text-xs ${
                recording
                  ? "bg-n-ruby-3 text-n-ruby-11"
                  : "text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
              }`}
            >
              {recording ? <Square className="size-3.5 fill-current" /> : <Mic className="size-4" />}
              {recording && <span className="font-mono">{recordSecs}s</span>}
            </button>
          </div>

          <button
            type="button"
            onClick={() => void send()}
            disabled={busy || (!text.trim() && files.length === 0)}
            className={`inline-flex h-8 items-center gap-1.5 rounded-lg px-3 text-xs font-medium text-white shadow-xs transition disabled:opacity-40 ${
              note
                ? "bg-n-amber-9 hover:bg-n-amber-10"
                : "bg-n-brand hover:bg-n-blue-10"
            }`}
          >
            <Send className="size-3.5" />
            <span>{note ? "Add Note" : "Send"}</span>
            <span className="hidden text-[10px] opacity-80 sm:inline">(⌘ + ↵)</span>
          </button>
        </div>
      </div>
    </div>
  );
}
