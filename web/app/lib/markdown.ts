// Zero-dependency HTML-safe Markdown renderer & HTML-to-Markdown serializer for composer and Help Center articles.
function escapeHtml(raw: string): string {
  return raw
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function unescapeHtml(raw: string): string {
  return raw
    .replaceAll("&nbsp;", " ")
    .replaceAll("&#39;", "'")
    .replaceAll("&quot;", '"')
    .replaceAll("&gt;", ">")
    .replaceAll("&lt;", "<")
    .replaceAll("&amp;", "&");
}

function safeHref(rawUrl: string): string | null {
  const trimmed = rawUrl.trim();
  if (/^https?:\/\//i.test(trimmed) || trimmed.startsWith("/")) {
    return trimmed.replaceAll('"', "&quot;").replaceAll("'", "&#39;");
  }
  return null;
}

function formatInline(escaped: string): string {
  // Inline code spans first so markers inside backticks stay literal.
  const codes: string[] = [];
  let s = escaped.replace(/`([^`]+)`/g, (_m, code: string) => {
    const idx = codes.length;
    codes.push(`<code class="rounded bg-n-alpha-2 px-1 py-0.5 font-mono text-[0.85em]">${code}</code>`);
    return `\u0000CODE${idx}\u0000`;
  });

  // Markdown links [text](url) — allow http(s) and relative paths only.
  s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_m, label: string, url: string) => {
    const href = safeHref(url.replaceAll("&amp;", "&"));
    if (!href) return `${label} (${url})`;
    return `<a href="${href}" target="_blank" rel="noopener noreferrer" class="text-n-brand underline">${label}</a>`;
  });

  s = s.replace(/~~([^~]+)~~/g, "<del>$1</del>");
  s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  s = s.replace(/(^|[\s(])_([^_]+)_/g, "$1<em>$2</em>");
  s = s.replace(/(^|[\s(])\*([^*\n]+)\*/g, "$1<em>$2</em>");

  return s.replace(/\u0000CODE(\d+)\u0000/g, (_m, i: string) => codes[Number(i)] ?? "");
}

export function renderSafeMarkdown(markdown: string): string {
  if (!markdown.trim()) return "";
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const out: string[] = [];
  let inCode = false;
  let codeLines: string[] = [];
  let listKind: "ul" | "ol" | null = null;

  const closeList = () => {
    if (listKind === "ul") {
      out.push("</ul>");
      listKind = null;
    } else if (listKind === "ol") {
      out.push("</ol>");
      listKind = null;
    }
  };

  for (const line of lines) {
    if (line.trimStart().startsWith("```")) {
      closeList();
      if (inCode) {
        out.push(
          `<pre class="my-2 overflow-x-auto rounded-lg bg-n-alpha-2 p-3 font-mono text-xs"><code>${escapeHtml(codeLines.join("\n"))}</code></pre>`,
        );
        codeLines = [];
        inCode = false;
      } else {
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      codeLines.push(line);
      continue;
    }

    const escaped = escapeHtml(line);
    const trimmed = escaped.trim();
    if (!trimmed) {
      closeList();
      continue;
    }
    if (trimmed.startsWith("### ")) {
      closeList();
      out.push(`<h3 class="mt-3 mb-1 text-base font-semibold">${formatInline(trimmed.slice(4))}</h3>`);
      continue;
    }
    if (trimmed.startsWith("## ")) {
      closeList();
      out.push(`<h2 class="mt-4 mb-1.5 text-lg font-semibold">${formatInline(trimmed.slice(3))}</h2>`);
      continue;
    }
    if (trimmed.startsWith("# ")) {
      closeList();
      out.push(`<h1 class="mt-4 mb-2 text-xl font-semibold">${formatInline(trimmed.slice(2))}</h1>`);
      continue;
    }
    if (trimmed.startsWith("- ") || trimmed.startsWith("* ")) {
      if (listKind !== "ul") {
        closeList();
        out.push('<ul class="my-1.5 list-disc space-y-1 pl-5">');
        listKind = "ul";
      }
      out.push(`<li>${formatInline(trimmed.slice(2))}</li>`);
      continue;
    }
    const orderedMatch = /^\d+\.\s+(.+)$/.exec(trimmed);
    if (orderedMatch) {
      if (listKind !== "ol") {
        closeList();
        out.push('<ol class="my-1.5 list-decimal space-y-1 pl-5">');
        listKind = "ol";
      }
      out.push(`<li>${formatInline(orderedMatch[1])}</li>`);
      continue;
    }
    closeList();
    if (trimmed.startsWith("&gt; ")) {
      out.push(
        `<blockquote class="my-2 border-l-2 border-n-weak pl-3 text-n-slate-11">${formatInline(trimmed.slice(5))}</blockquote>`,
      );
      continue;
    }
    out.push(`<p class="my-1">${formatInline(escaped)}</p>`);
  }

  closeList();
  if (inCode) {
    out.push(
      `<pre class="my-2 overflow-x-auto rounded-lg bg-n-alpha-2 p-3 font-mono text-xs"><code>${escapeHtml(codeLines.join("\n"))}</code></pre>`,
    );
  }
  return out.join("\n");
}

function stripTags(raw: string): string {
  return unescapeHtml(raw.replace(/<br\s*\/?>/gi, "\n").replace(/<[^>]+>/g, ""));
}

/** Converts contentEditable HTML back into clean, XSS-safe Markdown without DOM runtime dependencies. */
export function htmlToMarkdown(html: string): string {
  if (!html.trim()) return "";
  let s = html.replace(/\r\n?/g, "\n");

  // Preserve fenced code blocks before processing inline code.
  const codeBlocks: string[] = [];
  s = s.replace(/<pre[^>]*>\s*<code[^>]*>([\s\S]*?)<\/code>\s*<\/pre>/gi, (_m, inner: string) => {
    const idx = codeBlocks.length;
    codeBlocks.push(`\n\`\`\`\n${stripTags(inner).trimEnd()}\n\`\`\`\n`);
    return `\u0000BLOCK${idx}\u0000`;
  });

  // Inline code spans.
  s = s.replace(/<code[^>]*>([\s\S]*?)<\/code>/gi, (_m, inner: string) => `\`${stripTags(inner)}\``);

  // Links — filter unsafe schemes cleanly.
  s = s.replace(/<a\b([^>]*)>([\s\S]*?)<\/a>/gi, (_m, attrs: string, inner: string) => {
    const hrefMatch = /\bhref\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(attrs);
    const label = stripTags(inner).trim() || "link";
    const href = hrefMatch ? safeHref(unescapeHtml(hrefMatch[1] ?? hrefMatch[2] ?? "")) : null;
    return href ? `[${label}](${href})` : label;
  });

  // Inline emphasis & strikethrough.
  s = s.replace(/<(?:strong|b)\b[^>]*>([\s\S]*?)<\/(?:strong|b)>/gi, (_m, inner: string) => {
    const t = inner.trim();
    return t ? `**${t}**` : "";
  });
  s = s.replace(/<(?:em|i)\b[^>]*>([\s\S]*?)<\/(?:em|i)>/gi, (_m, inner: string) => {
    const t = inner.trim();
    return t ? `_${t}_` : "";
  });
  s = s.replace(/<(?:del|s|strike)\b[^>]*>([\s\S]*?)<\/(?:del|s|strike)>/gi, (_m, inner: string) => {
    const t = inner.trim();
    return t ? `~~${t}~~` : "";
  });

  // Ordered and unordered lists.
  s = s.replace(/<ol\b[^>]*>([\s\S]*?)<\/ol>/gi, (_m, body: string) => {
    let idx = 0;
    const items = body.replace(/<li\b[^>]*>([\s\S]*?)<\/li>/gi, (_im, li: string) => {
      idx += 1;
      return `${idx}. ${stripTags(li).trim()}\n`;
    });
    return `\n${items}\n`;
  });
  s = s.replace(/<ul\b[^>]*>([\s\S]*?)<\/ul>/gi, (_m, body: string) => {
    const items = body.replace(/<li\b[^>]*>([\s\S]*?)<\/li>/gi, (_im, li: string) => {
      return `- ${stripTags(li).trim()}\n`;
    });
    return `\n${items}\n`;
  });

  // Headings & blockquotes.
  s = s.replace(/<h1\b[^>]*>([\s\S]*?)<\/h1>/gi, (_m, inner: string) => `\n# ${stripTags(inner).trim()}\n`);
  s = s.replace(/<h2\b[^>]*>([\s\S]*?)<\/h2>/gi, (_m, inner: string) => `\n## ${stripTags(inner).trim()}\n`);
  s = s.replace(/<h3\b[^>]*>([\s\S]*?)<\/h3>/gi, (_m, inner: string) => `\n### ${stripTags(inner).trim()}\n`);
  s = s.replace(/<blockquote\b[^>]*>([\s\S]*?)<\/blockquote>/gi, (_m, inner: string) => {
    const lines = stripTags(inner)
      .trim()
      .split("\n")
      .map((l) => `> ${l.trim()}`);
    return `\n${lines.join("\n")}\n`;
  });

  // Paragraphs, divs, breaks, remaining tags.
  s = s.replace(/<br\s*\/?>/gi, "\n");
  s = s.replace(/<\/(?:p|div)>/gi, "\n");
  s = s.replace(/<(?:p|div)\b[^>]*>/gi, "");
  s = s.replace(/<[^>]+>/g, "");
  s = unescapeHtml(s);
  s = s.replace(/\u0000BLOCK(\d+)\u0000/g, (_m, i: string) => codeBlocks[Number(i)] ?? "");
  return s.replace(/\n{3,}/g, "\n\n").trim();
}
