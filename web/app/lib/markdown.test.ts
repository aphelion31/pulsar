import { describe, expect, test } from "bun:test";
import { htmlToMarkdown, renderSafeMarkdown } from "./markdown";

describe("markdown", () => {
  test("renders headings, bold, italic, strikethrough, code, lists, and safe links", () => {
    const html = renderSafeMarkdown(
      "# Heading\n\n**Bold** _ital_ ~~old~~ `code`\n\n- item 1\n- [Docs](https://example.com)\n\n1. first\n2. second",
    );
    expect(html).toContain("<h1");
    expect(html).toContain("<strong>Bold</strong>");
    expect(html).toContain("<em>ital</em>");
    expect(html).toContain("<del>old</del>");
    expect(html).toContain("<code");
    expect(html).toContain("<ul");
    expect(html).toContain("<ol");
    expect(html).toContain('<a href="https://example.com"');
  });

  test("escapes HTML tags and strips javascript URLs", () => {
    const html = renderSafeMarkdown('<script>alert(1)</script> [bad](javascript:alert(1))');
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toContain('href="javascript:');
  });

  test("converts WYSIWYG HTML back into clean Markdown safely", () => {
    const md = htmlToMarkdown(
      '<h2>Guide</h2><p><strong>Hello</strong> <em>world</em> <s>nope</s> <code>cmd</code></p><ul><li>One</li><li><a href="https://docs.example.com">Docs</a></li></ul><ol><li>Step A</li><li>Step B</li></ol><p><a href="javascript:alert(1)">Bad</a></p>',
    );
    expect(md).toContain("## Guide");
    expect(md).toContain("**Hello** _world_ ~~nope~~ `cmd`");
    expect(md).toContain("- One");
    expect(md).toContain("- [Docs](https://docs.example.com)");
    expect(md).toContain("1. Step A");
    expect(md).toContain("2. Step B");
    expect(md).not.toContain("javascript:");
  });
});
