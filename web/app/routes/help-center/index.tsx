import { BookOpen, Globe, Plus, RefreshCw, Settings, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useOutletContext, useParams } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import type { ShellCtx } from "../shell";

type LocaleInfo = {
  code: string;
  articles_count?: number;
  categories_count?: number;
  draft?: boolean;
};

type Portal = {
  id: number;
  name: string;
  slug: string;
  page_title?: string | null;
  header_text?: string | null;
  homepage_link?: string | null;
  custom_domain?: string | null;
  color?: string | null;
  archived?: boolean;
  config?: {
    allowed_locales?: LocaleInfo[];
    default_locale?: string;
    layout?: string;
    disable_search_indexing?: boolean;
    social_profiles?: Record<string, string>;
  };
  meta?: {
    all_articles_count?: number;
    published_count?: number;
    draft_articles_count?: number;
    archived_articles_count?: number;
    categories_count?: number;
    default_locale?: string;
  };
};

type Category = {
  id: number;
  name: string;
  slug: string;
  locale: string;
  description?: string | null;
  position?: number | null;
  icon?: string | null;
  meta?: { articles_count?: number };
};

type Article = {
  id: number;
  title: string;
  slug: string;
  status: string;
  locale: string;
  views?: number;
  updated_at?: number;
  category?: { id?: number | null; name?: string | null; slug?: string | null } | null;
  author?: { id?: number; name?: string } | null;
};

export default function HelpCenterDashboard() {
  const { accountId } = useOutletContext<ShellCtx>();
  const { portalSlug, locale: routeLocale, categorySlug, tab } = useParams();
  const location = useLocation();
  const navigate = useNavigate();

  const isNewPortal = location.pathname.endsWith("/portals/new");
  const isSettings = Boolean(portalSlug) && location.pathname.endsWith("/settings");
  const isLocales = Boolean(portalSlug) && location.pathname.endsWith("/locales");
  const isCategories = Boolean(portalSlug) && location.pathname.endsWith("/categories");
  const isArticles =
    Boolean(portalSlug) && !isSettings && !isLocales && !isCategories && !isNewPortal;

  const base = acct(accountId);
  const [portals, setPortals] = useState<Portal[]>([]);
  const [portal, setPortal] = useState<Portal | null>(null);
  const [categories, setCategories] = useState<Category[]>([]);
  const [articles, setArticles] = useState<Article[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  // Create portal fields
  const [newName, setNewName] = useState("");
  const [newSlug, setNewSlug] = useState("");
  const [newHeaderText, setNewHeaderText] = useState("");
  const [newPageTitle, setNewPageTitle] = useState("");
  const [newColor, setNewColor] = useState("#1f93ff");
  const [newHomepage, setNewHomepage] = useState("");
  const [newCustomDomain, setNewCustomDomain] = useState("");

  // Settings form
  const [editName, setEditName] = useState("");
  const [editHeader, setEditHeader] = useState("");
  const [editTitle, setEditTitle] = useState("");
  const [editHomepage, setEditHomepage] = useState("");
  const [editDomain, setEditDomain] = useState("");
  const [editColor, setEditColor] = useState("#1f93ff");
  const [editLayout, setEditLayout] = useState("classic");
  const [editNoIndex, setEditNoIndex] = useState(false);
  const [dnsEmail, setDnsEmail] = useState("");
  const [sslStatus, setSslStatus] = useState<{ status?: string | null } | null>(null);

  // Locales form
  const [localeCode, setLocaleCode] = useState("es");

  // Categories form
  const [catName, setCatName] = useState("");
  const [catSlug, setCatSlug] = useState("");
  const [catDesc, setCatDesc] = useState("");
  const [editingCat, setEditingCat] = useState<Category | null>(null);

  // Articles filters + bulk actions
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [translateModal, setTranslateModal] = useState(false);
  const [translateLocale, setTranslateLocale] = useState("");
  const [duplicateConflicts, setDuplicateConflicts] = useState<{ id: number; title: string }[]>([]);

  const activeLocale =
    routeLocale ||
    portal?.config?.default_locale ||
    portal?.meta?.default_locale ||
    "en";

  const loadPortals = useCallback(async () => {
    const res = await api<{ payload: Portal[] }>(`${base}/portals`);
    const list = res.payload || [];
    setPortals(list);
    return list;
  }, [base]);

  const loadPortalDetails = useCallback(
    async (slug: string, loc: string) => {
      const p = await api<Portal>(`${base}/portals/${slug}?locale=${encodeURIComponent(loc)}`);
      setPortal(p);
      setEditName(p.name || "");
      setEditHeader(p.header_text || "");
      setEditTitle(p.page_title || "");
      setEditHomepage(p.homepage_link || "");
      setEditDomain(p.custom_domain || "");
      setEditColor(p.color || "#1f93ff");
      setEditLayout(p.config?.layout || "classic");
      setEditNoIndex(Boolean(p.config?.disable_search_indexing));

      const catRes = await api<{ payload: Category[] }>(
        `${base}/portals/${slug}/categories?locale=${encodeURIComponent(loc)}`
      );
      setCategories(catRes.payload || []);

      const params = new URLSearchParams({ locale: loc });
      if (categorySlug) params.set("category_slug", categorySlug);
      if (tab === "draft") params.set("status", "0");
      else if (tab === "published") params.set("status", "1");
      else if (tab === "archived") params.set("status", "2");
      if (searchQuery.trim()) params.set("query", searchQuery.trim());

      const artRes = await api<{ payload: Article[] }>(
        `${base}/portals/${slug}/articles?${params.toString()}`
      );
      setArticles(artRes.payload || []);
    },
    [base, categorySlug, tab, searchQuery]
  );

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    (async () => {
      const list = await loadPortals();
      if (!active) return;
      if (portalSlug) {
        await loadPortalDetails(portalSlug, routeLocale || "en");
      } else if (!isNewPortal && list.length > 0 && location.pathname.endsWith("/portals")) {
        const first = list[0];
        const def = first.config?.default_locale || first.meta?.default_locale || "en";
        navigate(`/accounts/${accountId}/portals/${first.slug}/${def}/articles`, { replace: true });
      }
    })()
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load Help Center");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [accountId, isNewPortal, loadPortalDetails, loadPortals, location.pathname, navigate, portalSlug, routeLocale]);

  async function handleCreatePortal(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const slug = (newSlug.trim() || newName.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, ""));
      const created = await api<Portal>(`${base}/portals`, {
        method: "POST",
        json: {
          portal: {
            name: newName.trim(),
            slug,
            header_text: newHeaderText.trim() || newName.trim(),
            page_title: newPageTitle.trim() || newName.trim(),
            color: newColor,
            homepage_link: newHomepage.trim() || null,
            custom_domain: newCustomDomain.trim() || null,
            config: { allowed_locales: ["en"], default_locale: "en" },
          },
        },
      });
      navigate(`/accounts/${accountId}/portals/${created.slug}/en/articles`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create portal");
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveSettings(e: React.FormEvent) {
    e.preventDefault();
    if (!portalSlug) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await api(`${base}/portals/${portalSlug}`, {
        method: "PATCH",
        json: {
          portal: {
            name: editName.trim(),
            header_text: editHeader.trim(),
            page_title: editTitle.trim(),
            homepage_link: editHomepage.trim() || null,
            custom_domain: editDomain.trim() || null,
            color: editColor,
            config: {
              ...(portal?.config || {}),
              allowed_locales: portal?.config?.allowed_locales?.map((l) => l.code) || ["en"],
              default_locale: portal?.config?.default_locale || "en",
              layout: editLayout,
              disable_search_indexing: editNoIndex,
            },
          },
        },
      });
      await loadPortalDetails(portalSlug, activeLocale);
      setNotice("Portal settings updated.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to update settings");
    } finally {
      setBusy(false);
    }
  }

  async function handleSendDnsInstructions() {
    if (!portalSlug || !dnsEmail.trim()) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const res = await api<{ message: string }>(`${base}/portals/${portalSlug}/send_instructions`, {
        method: "POST",
        json: { email: dnsEmail.trim() },
      });
      setNotice(res.message || "Instructions sent.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to send instructions");
    } finally {
      setBusy(false);
    }
  }

  async function handleCheckSsl() {
    if (!portalSlug) return;
    setError("");
    try {
      const res = await api<{ status?: string | null }>(`${base}/portals/${portalSlug}/ssl_status`);
      setSslStatus(res);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to check SSL status");
    }
  }

  async function handleAddLocale(e: React.FormEvent) {
    e.preventDefault();
    if (!portalSlug || !localeCode.trim()) return;
    setBusy(true);
    setError("");
    try {
      const existing = portal?.config?.allowed_locales?.map((l) => l.code) || ["en"];
      const code = localeCode.trim().toLowerCase();
      const next = Array.from(new Set([...existing, code]));
      await api(`${base}/portals/${portalSlug}`, {
        method: "PATCH",
        json: {
          portal: {
            config: {
              allowed_locales: next,
              default_locale: portal?.config?.default_locale || "en",
            },
          },
        },
      });
      await loadPortalDetails(portalSlug, activeLocale);
      setNotice(`Locale '${code}' added.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to add locale");
    } finally {
      setBusy(false);
    }
  }

  async function handleSetDefaultLocale(code: string) {
    if (!portalSlug) return;
    setBusy(true);
    setError("");
    try {
      const existing = portal?.config?.allowed_locales?.map((l) => l.code) || ["en"];
      await api(`${base}/portals/${portalSlug}`, {
        method: "PATCH",
        json: {
          portal: {
            config: {
              allowed_locales: existing,
              default_locale: code,
            },
          },
        },
      });
      await loadPortalDetails(portalSlug, activeLocale);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to update default locale");
    } finally {
      setBusy(false);
    }
  }

  async function handleRemoveLocale(code: string) {
    if (!portalSlug) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/portals/${portalSlug}/remove_locale?locale=${encodeURIComponent(code)}`, {
        method: "DELETE",
      });
      await loadPortalDetails(portalSlug, portal?.config?.default_locale || "en");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to remove locale");
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveCategory(e: React.FormEvent) {
    e.preventDefault();
    if (!portalSlug || !catName.trim()) return;
    setBusy(true);
    setError("");
    try {
      const slug =
        catSlug.trim() ||
        catName.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
      if (editingCat) {
        await api(`${base}/portals/${portalSlug}/categories/${editingCat.id}`, {
          method: "PATCH",
          json: { category: { name: catName.trim(), slug, description: catDesc.trim(), locale: activeLocale } },
        });
      } else {
        await api(`${base}/portals/${portalSlug}/categories`, {
          method: "POST",
          json: {
            category: {
              name: catName.trim(),
              slug,
              description: catDesc.trim(),
              locale: activeLocale,
              position: categories.length + 1,
            },
          },
        });
      }
      setCatName("");
      setCatSlug("");
      setCatDesc("");
      setEditingCat(null);
      await loadPortalDetails(portalSlug, activeLocale);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save category");
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteCategory(id: number) {
    if (!portalSlug) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/portals/${portalSlug}/categories/${id}`, { method: "DELETE" });
      await loadPortalDetails(portalSlug, activeLocale);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete category");
    } finally {
      setBusy(false);
    }
  }

  async function runBulkStatus(status: "draft" | "published" | "archived") {
    if (!portalSlug || selectedIds.length === 0) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/portals/${portalSlug}/articles/bulk_actions/update_status`, {
        method: "PATCH",
        json: { ids: selectedIds, status },
      });
      setSelectedIds([]);
      await loadPortalDetails(portalSlug, activeLocale);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Bulk update failed");
    } finally {
      setBusy(false);
    }
  }

  async function runBulkDelete() {
    if (!portalSlug || selectedIds.length === 0) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/portals/${portalSlug}/articles/bulk_actions/delete_articles`, {
        method: "DELETE",
        json: { ids: selectedIds },
      });
      setSelectedIds([]);
      await loadPortalDetails(portalSlug, activeLocale);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Bulk delete failed");
    } finally {
      setBusy(false);
    }
  }

  async function runBulkTranslate(force = false) {
    if (!portalSlug || selectedIds.length === 0 || !translateLocale) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/portals/${portalSlug}/articles/bulk_actions/translate`, {
        method: "POST",
        json: { ids: selectedIds, locale: translateLocale, force },
      });
      setTranslateModal(false);
      setDuplicateConflicts([]);
      setSelectedIds([]);
      setNotice(`Translated ${selectedIds.length} article(s) to ${translateLocale}.`);
      await loadPortalDetails(portalSlug, activeLocale);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : "Translate failed";
      if (msg.includes("duplicate_articles")) {
        setDuplicateConflicts(selectedIds.map((id) => ({ id, title: `Article #${id}` })));
      } else {
        setError(msg);
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleArchivePortal(slug: string) {
    setBusy(true);
    try {
      await api(`${base}/portals/${slug}/archive`, { method: "PATCH" });
      await loadPortals();
      setNotice(`Portal '${slug}' archived.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Archive failed");
    } finally {
      setBusy(false);
    }
  }

  async function handleDeletePortal(slug: string) {
    setBusy(true);
    try {
      await api(`${base}/portals/${slug}`, { method: "DELETE" });
      const rest = await loadPortals();
      if (rest.length > 0) {
        navigate(`/accounts/${accountId}/portals/${rest[0].slug}/${rest[0].config?.default_locale || "en"}/articles`);
      } else {
        navigate(`/accounts/${accountId}/portals`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Delete failed");
    } finally {
      setBusy(false);
    }
  }

  const allowedLocales = portal?.config?.allowed_locales || [{ code: "en" }];

  return (
    <div className="flex h-full flex-1 flex-col overflow-y-auto bg-n-surface-1 p-6 sm:p-8 text-n-slate-12">
      {/* Top Bar */}
      <header className="mb-6 flex flex-wrap items-center justify-between gap-4 border-b border-n-weak pb-4">
        <div className="flex flex-wrap items-center gap-3">
          <BookOpen className="size-5 text-n-blue-11" />
          <h1 className="text-xl font-medium">Help Center</h1>
          {portals.length > 0 && portalSlug && (
            <select
              aria-label="Select portal"
              value={portalSlug}
              onChange={(e) => {
                const next = portals.find((p) => p.slug === e.target.value);
                const loc = next?.config?.default_locale || "en";
                navigate(`/accounts/${accountId}/portals/${e.target.value}/${loc}/articles`);
              }}
              className="h-9 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12"
            >
              {portals.map((p) => (
                <option key={p.id} value={p.slug}>
                  {p.name} ({p.slug})
                </option>
              ))}
            </select>
          )}
          {portalSlug && (
            <select
              aria-label="Select locale"
              value={activeLocale}
              onChange={(e) =>
                navigate(`/accounts/${accountId}/portals/${portalSlug}/${e.target.value}/articles`)
              }
              className="h-9 rounded-lg border border-n-weak bg-n-background px-3 text-sm text-n-slate-12"
            >
              {allowedLocales.map((l) => (
                <option key={l.code} value={l.code}>
                  {l.code.toUpperCase()} ({l.articles_count ?? 0})
                </option>
              ))}
            </select>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {portalSlug && (
            <>
              <Link
                to={`/accounts/${accountId}/portals/${portalSlug}/${activeLocale}/articles`}
                className={`rounded-lg px-3 py-1.5 text-sm ${isArticles ? "bg-n-alpha-2 font-medium text-n-slate-12" : "text-n-slate-11 hover:bg-n-alpha-1"}`}
              >
                Articles
              </Link>
              <Link
                to={`/accounts/${accountId}/portals/${portalSlug}/${activeLocale}/categories`}
                className={`rounded-lg px-3 py-1.5 text-sm ${isCategories ? "bg-n-alpha-2 font-medium text-n-slate-12" : "text-n-slate-11 hover:bg-n-alpha-1"}`}
              >
                Categories
              </Link>
              <Link
                to={`/accounts/${accountId}/portals/${portalSlug}/locales`}
                className={`rounded-lg px-3 py-1.5 text-sm ${isLocales ? "bg-n-alpha-2 font-medium text-n-slate-12" : "text-n-slate-11 hover:bg-n-alpha-1"}`}
              >
                Locales
              </Link>
              <Link
                to={`/accounts/${accountId}/portals/${portalSlug}/settings`}
                className={`inline-flex items-center gap-1 rounded-lg px-3 py-1.5 text-sm ${isSettings ? "bg-n-alpha-2 font-medium text-n-slate-12" : "text-n-slate-11 hover:bg-n-alpha-1"}`}
              >
                <Settings className="size-3.5" /> Settings
              </Link>
              <a
                href={`/hc/${portalSlug}/sitemap.xml`}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 rounded-lg border border-n-weak px-3 py-1.5 text-xs text-n-slate-11 hover:text-n-slate-12"
              >
                <Globe className="size-3.5" /> Sitemap
              </a>
            </>
          )}
          <Link to={`/accounts/${accountId}/portals/new`}>
            <Button solid blue sm label="New Portal" />
          </Link>
        </div>
      </header>

      {error && (
        <div role="alert" className="mb-4 rounded-lg border border-n-ruby-7 bg-n-ruby-2 p-3 text-sm text-n-ruby-11">
          {error}
        </div>
      )}
      {notice && (
        <div className="mb-4 rounded-lg border border-n-teal-7 bg-n-teal-2 p-3 text-sm text-n-teal-11">
          {notice}
        </div>
      )}

      {loading ? (
        <p className="text-sm text-n-slate-11">Loading Help Center...</p>
      ) : isNewPortal || (!portalSlug && portals.length === 0) ? (
        <form onSubmit={handleCreatePortal} className="max-w-xl space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6">
          <h2 className="text-lg font-medium">Create Help Center Portal</h2>
          <label className="flex flex-col gap-1 text-sm">
            <span>Portal Name *</span>
            <input
              required
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Acme Help Center"
              className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span>Slug *</span>
            <input
              value={newSlug}
              onChange={(e) => setNewSlug(e.target.value)}
              placeholder="acme-help"
              className="h-10 rounded-lg border border-n-weak bg-n-background px-3 font-mono text-xs"
            />
          </label>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <label className="flex flex-col gap-1 text-sm">
              <span>Header Text</span>
              <input
                value={newHeaderText}
                onChange={(e) => setNewHeaderText(e.target.value)}
                placeholder="How can we help?"
                className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span>Page Title</span>
              <input
                value={newPageTitle}
                onChange={(e) => setNewPageTitle(e.target.value)}
                placeholder="Acme Docs"
                className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
              />
            </label>
          </div>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <label className="flex flex-col gap-1 text-sm">
              <span>Custom Domain</span>
              <input
                value={newCustomDomain}
                onChange={(e) => setNewCustomDomain(e.target.value)}
                placeholder="help.example.com"
                className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span>Homepage URL</span>
              <input
                value={newHomepage}
                onChange={(e) => setNewHomepage(e.target.value)}
                placeholder="https://example.com"
                className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
              />
            </label>
          </div>
          <label className="flex items-center gap-3 text-sm">
            <span>Brand Color</span>
            <input
              type="color"
              value={newColor}
              onChange={(e) => setNewColor(e.target.value)}
              className="h-8 w-12 rounded border border-n-weak bg-transparent"
            />
          </label>
          <div className="flex justify-end gap-2 pt-2">
            <Button solid blue type="submit" isLoading={busy} label="Create Portal" />
          </div>
        </form>
      ) : isSettings && portal ? (
        <div className="grid max-w-4xl grid-cols-1 gap-6 lg:grid-cols-3">
          <form onSubmit={handleSaveSettings} className="space-y-4 rounded-xl border border-n-weak bg-n-solid-1 p-6 lg:col-span-2">
            <h2 className="text-lg font-medium">Portal Settings</h2>
            <label className="flex flex-col gap-1 text-sm">
              <span>Portal Name</span>
              <input value={editName} onChange={(e) => setEditName(e.target.value)} className="h-10 rounded-lg border border-n-weak bg-n-background px-3" />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span>Header Text</span>
              <input value={editHeader} onChange={(e) => setEditHeader(e.target.value)} className="h-10 rounded-lg border border-n-weak bg-n-background px-3" />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span>Page Title</span>
              <input value={editTitle} onChange={(e) => setEditTitle(e.target.value)} className="h-10 rounded-lg border border-n-weak bg-n-background px-3" />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span>Homepage Link</span>
              <input value={editHomepage} onChange={(e) => setEditHomepage(e.target.value)} className="h-10 rounded-lg border border-n-weak bg-n-background px-3" />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span>Custom Domain</span>
              <input value={editDomain} onChange={(e) => setEditDomain(e.target.value)} className="h-10 rounded-lg border border-n-weak bg-n-background px-3" />
            </label>
            <div className="grid grid-cols-2 gap-4">
              <label className="flex flex-col gap-1 text-sm">
                <span>Layout</span>
                <select value={editLayout} onChange={(e) => setEditLayout(e.target.value)} className="h-10 rounded-lg border border-n-weak bg-n-background px-3">
                  <option value="classic">Classic</option>
                  <option value="documentation">Documentation</option>
                </select>
              </label>
              <label className="flex flex-col gap-1 text-sm">
                <span>Color</span>
                <input type="color" value={editColor} onChange={(e) => setEditColor(e.target.value)} className="h-10 w-16 rounded border border-n-weak bg-transparent" />
              </label>
            </div>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={editNoIndex} onChange={(e) => setEditNoIndex(e.target.checked)} />
              <span>Disable search engine indexing (exclude from sitemap.xml)</span>
            </label>
            <div className="flex justify-end">
              <Button solid blue type="submit" isLoading={busy} label="Save Settings" />
            </div>
          </form>

          <div className="space-y-4">
            <div className="space-y-3 rounded-xl border border-n-weak bg-n-solid-1 p-5">
              <h3 className="text-sm font-medium">Custom Domain & SSL</h3>
              <input
                type="email"
                value={dnsEmail}
                onChange={(e) => setDnsEmail(e.target.value)}
                placeholder="admin@example.com"
                className="h-9 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm"
              />
              <div className="flex flex-wrap gap-2">
                <Button outline slate sm onClick={handleSendDnsInstructions} label="Send DNS Instructions" />
                <Button outline slate sm onClick={handleCheckSsl} label="Check SSL" />
              </div>
              {sslStatus && (
                <p className="text-xs text-n-slate-11">SSL status: {sslStatus.status || "Pending"}</p>
              )}
            </div>
            <div className="space-y-3 rounded-xl border border-n-weak bg-n-solid-1 p-5">
              <h3 className="text-sm font-medium text-n-ruby-11">Danger Zone</h3>
              <div className="flex flex-wrap gap-2">
                <Button outline slate sm onClick={() => handleArchivePortal(portal.slug)} label="Archive Portal" />
                <Button solid ruby sm onClick={() => handleDeletePortal(portal.slug)} label="Delete Portal" />
              </div>
            </div>
          </div>
        </div>
      ) : isLocales && portal ? (
        <div className="max-w-3xl space-y-6">
          <form onSubmit={handleAddLocale} className="flex flex-wrap items-end gap-3 rounded-xl border border-n-weak bg-n-solid-1 p-5">
            <label className="flex flex-col gap-1 text-sm">
              <span>Add Locale Code</span>
              <input
                value={localeCode}
                onChange={(e) => setLocaleCode(e.target.value)}
                placeholder="es, fr, id, ja..."
                className="h-10 rounded-lg border border-n-weak bg-n-background px-3"
              />
            </label>
            <Button solid blue type="submit" isLoading={busy} label="Add Locale" />
          </form>

          <div className="divide-y divide-n-weak rounded-xl border border-n-weak bg-n-solid-1">
            {allowedLocales.map((loc) => {
              const isDef = loc.code === (portal.config?.default_locale || "en");
              return (
                <div key={loc.code} className="flex flex-wrap items-center justify-between gap-4 p-4">
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-sm font-semibold uppercase">{loc.code}</span>
                      {isDef && (
                        <span className="rounded bg-n-blue-2 px-2 py-0.5 text-xs text-n-blue-11">Default</span>
                      )}
                    </div>
                    <p className="text-xs text-n-slate-11">
                      {loc.articles_count ?? 0} articles · {loc.categories_count ?? 0} categories
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    {!isDef && (
                      <>
                        <Button outline slate sm onClick={() => handleSetDefaultLocale(loc.code)} label="Set Default" />
                        <Button outline ruby sm onClick={() => handleRemoveLocale(loc.code)} label="Remove" />
                      </>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      ) : isCategories ? (
        <div className="grid max-w-5xl grid-cols-1 gap-6 lg:grid-cols-3">
          <form onSubmit={handleSaveCategory} className="space-y-3 rounded-xl border border-n-weak bg-n-solid-1 p-5 h-fit">
            <h2 className="text-base font-medium">
              {editingCat ? `Edit Category: ${editingCat.name}` : `New Category (${activeLocale})`}
            </h2>
            <input
              required
              value={catName}
              onChange={(e) => setCatName(e.target.value)}
              placeholder="Category name"
              className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 text-sm"
            />
            <input
              value={catSlug}
              onChange={(e) => setCatSlug(e.target.value)}
              placeholder="slug (auto)"
              className="h-10 w-full rounded-lg border border-n-weak bg-n-background px-3 font-mono text-xs"
            />
            <textarea
              rows={3}
              value={catDesc}
              onChange={(e) => setCatDesc(e.target.value)}
              placeholder="Description"
              className="w-full rounded-lg border border-n-weak bg-n-background p-3 text-sm"
            />
            <div className="flex gap-2">
              <Button solid blue type="submit" isLoading={busy} label={editingCat ? "Update" : "Add Category"} />
              {editingCat && (
                <Button
                  outline
                  slate
                  onClick={() => {
                    setEditingCat(null);
                    setCatName("");
                    setCatSlug("");
                    setCatDesc("");
                  }}
                  label="Cancel"
                />
              )}
            </div>
          </form>

          <div className="space-y-3 lg:col-span-2">
            {categories.length === 0 ? (
              <p className="rounded-xl border border-n-weak bg-n-solid-1 p-6 text-sm text-n-slate-11">
                No categories in locale {activeLocale} yet.
              </p>
            ) : (
              categories.map((c) => (
                <div key={c.id} className="flex items-center justify-between gap-4 rounded-xl border border-n-weak bg-n-solid-1 p-4">
                  <div className="min-w-0">
                    <Link
                      to={`/accounts/${accountId}/portals/${portalSlug}/${activeLocale}/categories/${c.slug}/articles`}
                      className="font-medium text-n-blue-11 hover:underline"
                    >
                      {c.name}
                    </Link>
                    <p className="text-xs text-n-slate-11">
                      /{c.slug} · {c.meta?.articles_count ?? 0} articles
                    </p>
                    {c.description && <p className="mt-1 text-xs text-n-slate-11">{c.description}</p>}
                  </div>
                  <div className="flex items-center gap-2">
                    <Button
                      outline
                      slate
                      sm
                      onClick={() => {
                        setEditingCat(c);
                        setCatName(c.name);
                        setCatSlug(c.slug);
                        setCatDesc(c.description || "");
                      }}
                      label="Edit"
                    />
                    <Button ghost ruby sm onClick={() => handleDeleteCategory(c.id)} label="Delete" />
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      ) : (
        /* ARTICLES LIST */
        <div className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-2">
              {(["all", "published", "draft", "archived"] as const).map((tKey) => {
                const isCurr = (tab || "all") === tKey;
                const to =
                  tKey === "all"
                    ? `/accounts/${accountId}/portals/${portalSlug}/${activeLocale}/articles`
                    : `/accounts/${accountId}/portals/${portalSlug}/${activeLocale}/articles/${tKey}`;
                return (
                  <Link
                    key={tKey}
                    to={to}
                    className={`rounded-lg px-3 py-1.5 text-sm capitalize ${
                      isCurr ? "bg-n-brand text-white" : "border border-n-weak bg-n-solid-1 text-n-slate-11"
                    }`}
                  >
                    {tKey}
                  </Link>
                );
              })}
              <input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Search articles..."
                className="h-9 rounded-lg border border-n-weak bg-n-background px-3 text-sm"
              />
            </div>

            <div className="flex items-center gap-2">
              <Button
                outline
                slate
                sm
                onClick={() => portalSlug && loadPortalDetails(portalSlug, activeLocale)}
                label="Refresh"
              >
                <RefreshCw className="size-3.5" />
              </Button>
              <Link
                to={`/accounts/${accountId}/portals/${portalSlug}/${activeLocale}/articles/new`}
              >
                <Button solid blue sm label="New Article">
                  <Plus className="size-4" />
                </Button>
              </Link>
            </div>
          </div>

          {selectedIds.length > 0 && (
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-n-blue-6 bg-n-blue-2 p-3 text-sm">
              <span className="font-medium">{selectedIds.length} selected</span>
              <div className="flex flex-wrap gap-2">
                <Button outline slate sm onClick={() => runBulkStatus("published")} label="Publish" />
                <Button outline slate sm onClick={() => runBulkStatus("draft")} label="Move to Draft" />
                <Button outline slate sm onClick={() => runBulkStatus("archived")} label="Archive" />
                <Button
                  outline
                  blue
                  sm
                  onClick={() => {
                    const other = allowedLocales.find((l) => l.code !== activeLocale)?.code || activeLocale;
                    setTranslateLocale(other);
                    setDuplicateConflicts([]);
                    setTranslateModal(true);
                  }}
                  label="Translate"
                />
                <Button solid ruby sm onClick={runBulkDelete} label="Delete" />
              </div>
            </div>
          )}

          {translateModal && (
            <div className="rounded-xl border border-n-weak bg-n-solid-1 p-4 space-y-3">
              <h3 className="font-medium text-sm">Translate {selectedIds.length} article(s)</h3>
              <div className="flex flex-wrap items-center gap-3">
                <select
                  value={translateLocale}
                  onChange={(e) => setTranslateLocale(e.target.value)}
                  className="h-9 rounded-lg border border-n-weak bg-n-background px-3 text-sm"
                >
                  {allowedLocales.map((l) => (
                    <option key={l.code} value={l.code}>
                      {l.code}
                    </option>
                  ))}
                </select>
                {duplicateConflicts.length > 0 ? (
                  <Button solid ruby sm onClick={() => runBulkTranslate(true)} label="Overwrite Existing Translations" />
                ) : (
                  <Button solid blue sm onClick={() => runBulkTranslate(false)} label="Create Translations" />
                )}
                <Button outline slate sm onClick={() => setTranslateModal(false)} label="Cancel" />
              </div>
            </div>
          )}

          <div className="overflow-x-auto rounded-xl border border-n-weak bg-n-solid-1">
            <table className="w-full text-left text-sm">
              <thead className="border-b border-n-weak bg-n-solid-2 text-xs uppercase text-n-slate-11">
                <tr>
                  <th className="w-10 px-4 py-3">
                    <input
                      type="checkbox"
                      checked={articles.length > 0 && selectedIds.length === articles.length}
                      onChange={(e) =>
                        setSelectedIds(e.target.checked ? articles.map((a) => a.id) : [])
                      }
                    />
                  </th>
                  <th className="px-4 py-3">Title</th>
                  <th className="px-4 py-3">Category</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Views</th>
                  <th className="px-4 py-3 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-n-weak">
                {articles.length === 0 ? (
                  <tr>
                    <td colSpan={6} className="px-4 py-8 text-center text-n-slate-11">
                      No articles found.
                    </td>
                  </tr>
                ) : (
                  articles.map((art) => (
                    <tr key={art.id} className="hover:bg-n-alpha-1">
                      <td className="px-4 py-3">
                        <input
                          type="checkbox"
                          checked={selectedIds.includes(art.id)}
                          onChange={(e) =>
                            setSelectedIds((prev) =>
                              e.target.checked ? [...prev, art.id] : prev.filter((x) => x !== art.id)
                            )
                          }
                        />
                      </td>
                      <td className="px-4 py-3 font-medium">
                        <Link
                          to={`/accounts/${accountId}/portals/${portalSlug}/${activeLocale}/articles/edit/${art.id}`}
                          className="text-n-blue-11 hover:underline"
                        >
                          {art.title}
                        </Link>
                        <div className="font-mono text-xs text-n-slate-10">/{art.slug}</div>
                      </td>
                      <td className="px-4 py-3 text-n-slate-11">{art.category?.name || "—"}</td>
                      <td className="px-4 py-3">
                        <span className="rounded bg-n-alpha-2 px-2 py-0.5 text-xs capitalize">
                          {art.status}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-n-slate-11">{art.views ?? 0}</td>
                      <td className="px-4 py-3 text-right">
                        <a
                          href={`/hc/${portalSlug}/articles/${art.slug}.md`}
                          target="_blank"
                          rel="noreferrer"
                          className="mr-3 text-xs text-n-slate-11 hover:underline"
                        >
                          .md
                        </a>
                        <button
                          type="button"
                          onClick={async () => {
                            if (!portalSlug) return;
                            await api(`${base}/portals/${portalSlug}/articles/${art.id}`, { method: "DELETE" });
                            await loadPortalDetails(portalSlug, activeLocale);
                          }}
                          className="inline-flex items-center text-n-ruby-11 hover:opacity-80"
                          aria-label={`Delete ${art.title}`}
                        >
                          <Trash2 className="size-4" />
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
