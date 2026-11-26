// Dual-mode zero-dependency Rich WYSIWYG (contentEditable) + Markdown + Split composer/editor.
import {
  Bold,
  Code,
  Columns2,
  Eye,
  FileCode2,
  Heading2,
  Italic,
  Link as LinkIcon,
  List,
  ListOrdered,
  Quote,
  Strikethrough,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { htmlToMarkdown, renderSafeMarkdown } from "~/lib/markdown";

export type EditorMode = "visual" | "markdown" | "split";

export interface RichTextEditorProps {
  value: string;
  onChange: (nextMarkdown: string) => void;
  mode?: EditorMode;
  onModeChange?: (nextMode: EditorMode) => void;
  defaultMode?: EditorMode;
  ariaLabel?: string;
  placeholder?: string;
  rows?: number;
  allowHeading?: boolean;
  allowSplit?: boolean;
  compact?: boolean;
  className?: string;
  minHeightClass?: string;
  textareaRef?: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown?: (e: React.KeyboardEvent) => void;
  trailingControls?: React.ReactNode;
}

export function RichTextEditor({
  value,
  onChange,
  mode: controlledMode,
  onModeChange,
  defaultMode = "visual",
  ariaLabel = "Editor",
  placeholder = "Write here...",
  rows = 4,
  allowHeading = false,
  allowSplit = true,
  compact = false,
  className = "",
  minHeightClass = "min-h-[84px]",
  textareaRef: externalTextareaRef,
  onKeyDown,
  trailingControls,
}: RichTextEditorProps) {
  const [internalMode, setInternalMode] = useState<EditorMode>(defaultMode);
  const mode = controlledMode ?? internalMode;
  const setMode = (next: EditorMode) => {
    if (controlledMode === undefined) setInternalMode(next);
    onModeChange?.(next);
  };

  const innerTextareaRef = useRef<HTMLTextAreaElement>(null);
  const taRef = externalTextareaRef ?? innerTextareaRef;
  const visualRef = useRef<HTMLDivElement>(null);
  // Track the Markdown representation last synchronized to the visual DOM so typing doesn't reset caret.
  const syncedMdRef = useRef<string>("");

  const showVisual = mode === "visual" || mode === "split";
  const showMarkdown = mode === "markdown" || mode === "split";

  useEffect(() => {
    const el = visualRef.current;
    if (!el || !showVisual) return;
    if (syncedMdRef.current === value && el.innerHTML !== "") return;
    const active = typeof document !== "undefined" && document.activeElement === el;
    // Allow external programmatic updates (clear on send, templates, switching modes) even if ref changed.
    if (!active || syncedMdRef.current !== value) {
      el.innerHTML = renderSafeMarkdown(value);
      syncedMdRef.current = value;
    }
  }, [value, showVisual, mode]);

  function handleVisualInput() {
    const el = visualRef.current;
    if (!el) return;
    const md = htmlToMarkdown(el.innerHTML);
    syncedMdRef.current = md;
    onChange(md);
  }

  function wrapMarkdown(before: string, after = "") {
    const el = taRef.current;
    if (!el) {
      onChange(`${value}${before}text${after}`);
      return;
    }
    const start = el.selectionStart ?? value.length;
    const end = el.selectionEnd ?? value.length;
    const selected = value.slice(start, end) || "text";
    const next = value.slice(0, start) + before + selected + after + value.slice(end);
    syncedMdRef.current = "";
    onChange(next);
    setTimeout(() => {
      el.focus();
      el.setSelectionRange(start + before.length, start + before.length + selected.length);
    }, 0);
  }

  function execVisualCommand(cmd: string, cmdVal?: string) {
    const el = visualRef.current;
    if (!el || typeof document === "undefined") return;
    el.focus();
    document.execCommand(cmd, false, cmdVal);
    handleVisualInput();
  }

  function wrapVisualInlineCode() {
    const el = visualRef.current;
    if (!el || typeof window === "undefined") return;
    el.focus();
    const sel = window.getSelection();
    const selected = sel?.toString().trim() || "code";
    const escaped = selected.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    document.execCommand("insertHTML", false, `<code>${escaped}</code>`);
    handleVisualInput();
  }

  function applyFormat(kind: "bold" | "italic" | "strike" | "heading" | "quote" | "code" | "link" | "ul" | "ol") {
    const useVisual =
      mode === "visual" ||
      (mode === "split" && typeof document !== "undefined" && document.activeElement !== taRef.current);

    if (!useVisual) {
      switch (kind) {
        case "bold":
          return wrapMarkdown("**", "**");
        case "italic":
          return wrapMarkdown("_", "_");
        case "strike":
          return wrapMarkdown("~~", "~~");
        case "heading":
          return wrapMarkdown("## ");
        case "quote":
          return wrapMarkdown("> ");
        case "code":
          return wrapMarkdown("`", "`");
        case "link":
          return wrapMarkdown("[", "](https://)");
        case "ul":
          return wrapMarkdown("- ");
        case "ol":
          return wrapMarkdown("1. ");
      }
    }

    switch (kind) {
      case "bold":
        return execVisualCommand("bold");
      case "italic":
        return execVisualCommand("italic");
      case "strike":
        return execVisualCommand("strikeThrough");
      case "heading":
        return execVisualCommand("formatBlock", "<h2>");
      case "quote":
        return execVisualCommand("formatBlock", "<blockquote>");
      case "code":
        return wrapVisualInlineCode();
      case "ul":
        return execVisualCommand("insertUnorderedList");
      case "ol":
        return execVisualCommand("insertOrderedList");
      case "link": {
        const url = typeof window !== "undefined" ? window.prompt("Enter URL (https://...)", "https://") : "https://";
        if (!url) return;
        const trimmed = url.trim();
        if (!/^https?:\/\//i.test(trimmed) && !trimmed.startsWith("/")) return;
        return execVisualCommand("createLink", trimmed);
      }
    }
  }

  const iconBtn = compact
    ? "grid size-6 place-items-center rounded hover:bg-n-alpha-2 hover:text-n-slate-12"
    : "grid size-7 place-items-center rounded hover:bg-n-alpha-2 hover:text-n-slate-12";

  return (
    <div data-editor-mode={mode} className={className}>
      <div
        className={`flex flex-wrap items-center justify-between gap-2 text-n-slate-10 ${
          compact ? "mb-1" : "border-b border-n-weak bg-n-solid-2 px-3 py-1.5"
        }`}
      >
        <div className="flex flex-wrap items-center gap-0.5">
          {allowHeading && (
            <button type="button" onClick={() => applyFormat("heading")} title="Heading" aria-label="Heading" className={iconBtn}>
              <Heading2 className={compact ? "size-3.5" : "size-4"} />
            </button>
          )}
          <button type="button" onClick={() => applyFormat("bold")} title="Bold" aria-label="Bold" className={iconBtn}>
            <Bold className={compact ? "size-3.5" : "size-4"} />
          </button>
          <button type="button" onClick={() => applyFormat("italic")} title="Italic" aria-label="Italic" className={iconBtn}>
            <Italic className={compact ? "size-3.5" : "size-4"} />
          </button>
          <button type="button" onClick={() => applyFormat("strike")} title="Strikethrough" aria-label="Strikethrough" className={iconBtn}>
            <Strikethrough className={compact ? "size-3.5" : "size-4"} />
          </button>
          <button type="button" onClick={() => applyFormat("code")} title="Code" aria-label="Inline code" className={iconBtn}>
            <Code className={compact ? "size-3.5" : "size-4"} />
          </button>
          <button type="button" onClick={() => applyFormat("link")} title="Link" aria-label="Insert link" className={iconBtn}>
            <LinkIcon className={compact ? "size-3.5" : "size-4"} />
          </button>
          <button type="button" onClick={() => applyFormat("quote")} title="Quote" aria-label="Quote" className={iconBtn}>
            <Quote className={compact ? "size-3.5" : "size-4"} />
          </button>
          <button type="button" onClick={() => applyFormat("ul")} title="Bullet list" aria-label="Bullet list" className={iconBtn}>
            <List className={compact ? "size-3.5" : "size-4"} />
          </button>
          <button type="button" onClick={() => applyFormat("ol")} title="Numbered list" aria-label="Numbered list" className={iconBtn}>
            <ListOrdered className={compact ? "size-3.5" : "size-4"} />
          </button>
        </div>

        <div className="flex items-center gap-1">
          <div className="inline-flex rounded-md border border-n-weak bg-n-alpha-1 p-0.5 text-[11px] font-medium">
            <button
              type="button"
              onClick={() => setMode("visual")}
              aria-pressed={mode === "visual"}
              className={`inline-flex items-center gap-1 rounded px-2 py-0.5 transition-colors ${
                mode === "visual" ? "bg-n-solid-1 text-n-slate-12 shadow-2xs" : "text-n-slate-10 hover:text-n-slate-12"
              }`}
            >
              <Eye className="size-3" />
              <span>Visual</span>
            </button>
            <button
              type="button"
              onClick={() => setMode("markdown")}
              aria-pressed={mode === "markdown"}
              className={`inline-flex items-center gap-1 rounded px-2 py-0.5 transition-colors ${
                mode === "markdown" ? "bg-n-solid-1 text-n-slate-12 shadow-2xs" : "text-n-slate-10 hover:text-n-slate-12"
              }`}
            >
              <FileCode2 className="size-3" />
              <span>Markdown</span>
            </button>
            {allowSplit && (
              <button
                type="button"
                onClick={() => setMode("split")}
                aria-pressed={mode === "split"}
                className={`inline-flex items-center gap-1 rounded px-2 py-0.5 transition-colors ${
                  mode === "split" ? "bg-n-solid-1 text-n-slate-12 shadow-2xs" : "text-n-slate-10 hover:text-n-slate-12"
                }`}
              >
                <Columns2 className="size-3" />
                <span>Split</span>
              </button>
            )}
          </div>
          {trailingControls}
        </div>
      </div>

      <div className={mode === "split" ? "grid grid-cols-1 gap-3 md:grid-cols-2" : ""}>
        {showMarkdown && (
          <textarea
            ref={taRef}
            aria-label={mode === "split" ? `${ariaLabel} Markdown` : ariaLabel}
            value={value}
            onChange={(e) => {
              syncedMdRef.current = "";
              onChange(e.target.value);
            }}
            onKeyDown={onKeyDown}
            rows={rows}
            placeholder={placeholder}
            className={`w-full resize-y bg-transparent text-sm leading-relaxed text-n-slate-12 outline-none placeholder:text-n-slate-10 ${
              compact ? "py-1 font-mono text-[13px]" : "p-4 font-mono"
            } ${mode === "split" ? "rounded-lg border border-n-weak p-3" : ""}`}
          />
        )}

        {showVisual && (
          <div className={`relative ${mode === "split" ? "rounded-lg border border-n-weak p-3" : compact ? "py-1" : "p-4"}`}>
            {!value.trim() && (
              <span
                className={`pointer-events-none absolute text-sm text-n-slate-10 select-none ${
                  mode === "split" ? "top-3 left-3" : compact ? "top-1 left-0" : "top-4 left-4"
                }`}
              >
                {placeholder}
              </span>
            )}
            <div
              ref={visualRef}
              contentEditable
              suppressContentEditableWarning
              role="textbox"
              aria-multiline="true"
              aria-label={mode === "split" ? `${ariaLabel} Visual` : ariaLabel}
              onInput={handleVisualInput}
              onBlur={handleVisualInput}
              onKeyDown={onKeyDown}
              className={`w-full overflow-y-auto text-sm leading-relaxed text-n-slate-12 outline-none [&_a]:text-n-brand [&_a]:underline [&_blockquote]:my-1.5 [&_blockquote]:border-l-2 [&_blockquote]:border-n-weak [&_blockquote]:pl-3 [&_code]:rounded [&_code]:bg-n-alpha-2 [&_code]:px-1 [&_h1]:text-xl [&_h1]:font-semibold [&_h2]:text-lg [&_h2]:font-semibold [&_h3]:text-base [&_h3]:font-semibold [&_ol]:list-decimal [&_ol]:pl-5 [&_ul]:list-disc [&_ul]:pl-5 ${minHeightClass} ${
                compact ? "max-h-60" : "max-h-[520px]"
              }`}
            />
          </div>
        )}
      </div>
    </div>
  );
}
