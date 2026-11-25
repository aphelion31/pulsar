import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { Button, ErrorText, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";

const DEFAULT_TEMPLATE = `<!DOCTYPE html>
<html>
  <body style="font-family: sans-serif; background: #f8fafc; padding: 24px;">
    <div style="max-width: 600px; margin: 0 auto; background: #ffffff; padding: 24px; border-radius: 8px;">
      {{ content_for_layout }}
    </div>
  </body>
</html>`;

export default function EmailTemplatesRoute() {
  const { accountId } = useOutletContext<ShellCtx>();
  const endpoint = `${acct(accountId)}/branded_email_layout`;
  const [layout, setLayout] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState("");

  useEffect(() => {
    let alive = true;
    setLoading(true);
    api<{ branded_email_layout: string | null }>(endpoint)
      .then((res) => {
        if (alive) setLayout(res.branded_email_layout ?? "");
      })
      .catch((e) => {
        if (alive) setError(e instanceof Error ? e.message : "Failed to load layout");
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [endpoint]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    setSaved("");
    try {
      const res = await api<{ branded_email_layout: string | null }>(endpoint, {
        method: "PUT",
        json: { branded_email_layout: layout.trim() ? layout : null },
      });
      setLayout(res.branded_email_layout ?? "");
      setSaved("Email template layout saved.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save layout");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title="Email Templates">
        <Button
          variant="quiet"
          type="button"
          onClick={() => setLayout(DEFAULT_TEMPLATE)}
        >
          Insert starter layout
        </Button>
      </PageHeader>
      <div className="flex-1 overflow-y-auto p-6">
        <p className="mb-4 max-w-2xl text-sm text-ink-2">
          Customize the branded HTML wrapper used for outgoing emails. Include{" "}
          <code className="rounded bg-raised px-1.5 py-0.5 text-xs">{"{{ content_for_layout }}"}</code>{" "}
          where the message body should be rendered. Leave empty to use the default layout.
        </p>
        {!loading && (
          <form onSubmit={save} className="max-w-3xl space-y-4">
            <textarea
              rows={14}
              value={layout}
              onChange={(e) => setLayout(e.target.value)}
              placeholder={DEFAULT_TEMPLATE}
              className="w-full rounded-lg border border-line-strong bg-surface p-3 font-mono text-xs text-ink outline-none focus:border-accent"
            />
            <ErrorText>{error}</ErrorText>
            {saved && <p role="status" className="text-sm text-ink-2">{saved}</p>}
            <Button type="submit" disabled={busy}>
              {busy ? "Saving..." : "Save template"}
            </Button>
          </form>
        )}
      </div>
    </section>
  );
}
