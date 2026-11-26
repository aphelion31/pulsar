import { describe, expect, test } from "bun:test";
import { renderSafeMarkdown } from "./markdown";

describe("renderSafeMarkdown", () => {
  test("renders headings, bold, italic, code, lists, and safe links", () => {
    const html = renderSafeMarkdown("# Heading\n\n**Bold** _ital_ `code`\n\n- item 1\n- [Docs](https://example.com)");
    expect(html).toContain("<h1");
    expect(html).toContain("<strong>Bold</strong>");
    expect(html).toContain("<em>ital</em>");
    expect(html).toContain("<code");
    expect(html).toContain("<ul");
    expect(html).toContain('<a href="https://example.com"');
  });

  test("escapes HTML tags and strips javascript URLs", () => {
    const html = renderSafeMarkdown('<script>alert(1)</script> [bad](javascript:alert(1))');
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toContain('href="javascript:');
  });
});
