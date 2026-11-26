import { ArrowLeft, Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate, useOutletContext, useParams } from "react-router";
import Button from "~/components/next/Button";
import { RichTextEditor } from "~/components/next/RichTextEditor";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";

type Category = { id: number; name: string; slug: string; locale: string };
type Article = {
  id: number;
  title: string;
  slug: string;
  content: string;
  description?: string | null;
  status: string;
  locale: string;
  category?: { id?: number | null } | null;
  meta?: { title?: string; description?: string; tags?: string[] } | null;
};

export default function HelpCenterArticleEditor() {
  const { me, accountId } = useOutletContext<ShellCtx>();
  const { portalSlug, locale = "en", categorySlug, articleSlug } = useParams();
  const navigate = useNavigate();
  const base = acct(accountId);
  const isEdit = Boolean(articleSlug);

  const [categories, setCategories] = useState<Category[]>([]);
  const [articleId, setArticleId] = useState<number | null>(null);
  const [title, setTitle] = useState("");
  const [slug, setSlug] = useState("");
  const [description, setDescription] = useState("");
  const [content, setContent] = useState("");
  const [categoryId, setCategoryId] = useState<string>("");
  const [status, setStatus] = useState<"draft" | "published" | "archived">("draft");
  const [metaTags, setMetaTags] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!portalSlug) return;
    let active = true;
    setLoading(true);
    setError("");
    (async () => {
      const catRes = await api<{ payload: Category[] }>(
        `${base}/portals/${portalSlug}/categories?locale=${encodeURIComponent(locale)}`
      );
      if (!active) return;
      const cats = catRes.payload || [];
      setCategories(cats);

      if (categorySlug) {
        const matched = cats.find((c) => c.slug === categorySlug);
        if (matched) setCategoryId(String(matched.id));
      }

      if (articleSlug) {
        let art: Article | undefined;
        if (/^\d+$/.test(articleSlug)) {
          const r = await api<Article | { payload: Article }>(`${base}/portals/${portalSlug}/articles/${articleSlug}`);
          art = "payload" in r ? r.payload : r;
        } else {
          const list = await api<{ payload: Article[] }>(
            `${base}/portals/${portalSlug}/articles?locale=${encodeURIComponent(locale)}`
          );
          art = (list.payload || []).find((a) => a.slug === articleSlug);
        }
        if (!active || !art) return;
        setArticleId(art.id);
        setTitle(art.title || "");
        setSlug(art.slug || "");
        setDescription(art.description || "");
        setContent(art.content || "");
        setCategoryId(art.category?.id ? String(art.category.id) : "");
        setStatus((art.status as "draft" | "published" | "archived") || "draft");
        setMetaTags((art.meta?.tags || []).join(", "));
      }
    })()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load article");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [articleSlug, base, categorySlug, locale, portalSlug]);

  async function saveArticle(nextStatus?: "draft" | "published" | "archived") {
    if (!portalSlug || !title.trim()) return;
    setSaving(true);
    setError("");
    const targetStatus = nextStatus ?? status;
    const statusInt = targetStatus === "published" ? 1 : targetStatus === "archived" ? 2 : 0;
    const resolvedSlug =
      slug.trim() ||
      title
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-|-$/g, "");
    try {
      const payload = {
        article: {
          title: title.trim(),
          slug: resolvedSlug,
          description: description.trim(),
          content,
          locale,
          author_id: me.id,
          category_id: categoryId ? Number(categoryId) : null,
          status: statusInt,
          meta: {
            tags: metaTags
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean),
          },
        },
      };
      if (isEdit && articleId) {
        const res = await api<Article | { payload: Article }>(`${base}/portals/${portalSlug}/articles/${articleId}`, {
          method: "PATCH",
          json: payload,
        });
        const updated = "payload" in res ? res.payload : res;
        setStatus((updated.status as "draft" | "published" | "archived") || targetStatus);
        navigate(`/accounts/${accountId}/portals/${portalSlug}/${locale}/articles`);
      } else {
        await api<Article>(`${base}/portals/${portalSlug}/articles`, {
          method: "POST",
          json: payload,
        });
        navigate(`/accounts/${accountId}/portals/${portalSlug}/${locale}/articles`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save article");
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!portalSlug || !articleId) return;
    setSaving(true);
    try {
      await api(`${base}/portals/${portalSlug}/articles/${articleId}`, { method: "DELETE" });
      navigate(`/accounts/${accountId}/portals/${portalSlug}/${locale}/articles`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete article");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex h-full flex-1 flex-col overflow-y-auto bg-n-surface-1 p-6 sm:p-8 text-n-slate-12">
      <header className="mb-6 flex flex-wrap items-center justify-between gap-4 border-b border-n-weak pb-4">
        <div className="flex items-center gap-3">
          <Link
            to={`/accounts/${accountId}/portals/${portalSlug}/${locale}/articles`}
            className="inline-flex items-center gap-1 text-sm text-n-slate-11 hover:text-n-slate-12"
          >
            <ArrowLeft className="size-4" /> Back to articles
          </Link>
          <span className="text-n-slate-9">/</span>
          <h1 className="text-lg font-medium">{isEdit ? "Edit Article" : "New Article"}</h1>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {isEdit && articleId && (
            <Button ghost ruby sm onClick={handleDelete} label="Delete">
              <Trash2 className="size-3.5" />
            </Button>
          )}
          <Button outline slate sm isLoading={saving} onClick={() => saveArticle("draft")} label="Save Draft" />
          <Button solid blue sm isLoading={saving} onClick={() => saveArticle("published")} label="Publish">
            <Save className="size-3.5" />
          </Button>
        </div>
      </header>

      {error && (
        <div role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">
          {error}
        </div>
      )}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading editor...</p>
      ) : (
        <div className="grid max-w-6xl grid-cols-1 gap-6 lg:grid-cols-3">
          <div className="space-y-4 lg:col-span-2">
            <input
              required
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Article title"
              className="w-full rounded-xl border border-n-weak bg-n-solid-1 px-4 py-3 text-xl font-medium outline-none focus:border-n-brand"
            />
            <div className="overflow-hidden rounded-xl border border-n-weak bg-n-solid-1">
              <RichTextEditor
                value={content}
                onChange={setContent}
                allowHeading
                allowSplit
                rows={18}
                minHeightClass="min-h-[420px]"
                ariaLabel="Article body"
                placeholder="Write article content..."
              />
            </div>
          </div>

          <aside className="space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-5 h-fit">
            <h2 className="text-sm font-medium">Article Settings</h2>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs text-n-slate-11">Slug</span>
              <input
                value={slug}
                onChange={(e) => setSlug(e.target.value)}
                placeholder="auto-from-title"
                className="h-9 rounded-lg border border-n-weak bg-n-background px-3 font-mono text-xs"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs text-n-slate-11">Category ({locale})</span>
              <select
                value={categoryId}
                onChange={(e) => setCategoryId(e.target.value)}
                className="h-9 rounded-lg border border-n-weak bg-n-background px-3 text-sm"
              >
                <option value="">Uncategorized</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs text-n-slate-11">Status</span>
              <select
                value={status}
                onChange={(e) => setStatus(e.target.value as "draft" | "published" | "archived")}
                className="h-9 rounded-lg border border-n-weak bg-n-background px-3 text-sm"
              >
                <option value="draft">Draft</option>
                <option value="published">Published</option>
                <option value="archived">Archived</option>
              </select>
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs text-n-slate-11">Summary / SEO Description</span>
              <textarea
                rows={3}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                className="rounded-lg border border-n-weak bg-n-background p-2 text-sm"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs text-n-slate-11">Tags (comma separated)</span>
              <input
                value={metaTags}
                onChange={(e) => setMetaTags(e.target.value)}
                placeholder="billing, setup"
                className="h-9 rounded-lg border border-n-weak bg-n-background px-3 text-sm"
              />
            </label>
          </aside>
        </div>
      )}
    </div>
  );
}
