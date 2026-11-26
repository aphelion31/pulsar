// Zero-dependency HTML-safe Markdown preview renderer for composer and Help Center articles.
function escapeHtml(raw: string): string {
  return raw
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
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
  let inList = false;

  const closeList = () => {
    if (inList) {
      out.push("</ul>");
      inList = false;
    }
  };

  for (const line of lines) {
    if (line.trimStart().startsWith("```")) {
      closeList();
      if (inCode) {
        out.push(`<pre class="my-2 overflow-x-auto rounded-lg bg-n-alpha-2 p-3 font-mono text-xs"><code>${escapeHtml(codeLines.join("\n"))}</code></pre>`);
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
      if (!inList) {
        out.push('<ul class="my-1.5 list-disc space-y-1 pl-5">');
        inList = true;
      }
      out.push(`<li>${formatInline(trimmed.slice(2))}</li>`);
      continue;
    }
    closeList();
    if (trimmed.startsWith("&gt; ")) {
      out.push(`<blockquote class="my-2 border-l-2 border-n-weak pl-3 text-n-slate-11">${formatInline(trimmed.slice(5))}</blockquote>`);
      continue;
    }
    out.push(`<p class="my-1">${formatInline(escaped)}</p>`);
  }

  closeList();
  if (inCode) {
    out.push(`<pre class="my-2 overflow-x-auto rounded-lg bg-n-alpha-2 p-3 font-mono text-xs"><code>${escapeHtml(codeLines.join("\n"))}</code></pre>`);
  }
  return out.join("\n");
}
