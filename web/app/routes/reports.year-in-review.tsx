import { ArrowLeft, Calendar, Flame, MessageSquare, Share2, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "./shell";
import { formatTime } from "./reports";

type YearInReviewData = {
  year: number;
  total_conversations: number;
  busiest_day: { date: string; count: number } | null;
  support_personality: { avg_response_time_seconds: number };
};

function volumeComparison(total: number): string {
  if (total < 50) return t("YEAR_IN_REVIEW.CONVERSATIONS.COMPARISON.0_50");
  if (total < 100) return t("YEAR_IN_REVIEW.CONVERSATIONS.COMPARISON.50_100");
  if (total < 500) return t("YEAR_IN_REVIEW.CONVERSATIONS.COMPARISON.100_500");
  if (total < 2000) return t("YEAR_IN_REVIEW.CONVERSATIONS.COMPARISON.500_2000");
  if (total < 10000) return t("YEAR_IN_REVIEW.CONVERSATIONS.COMPARISON.2000_10000");
  return t("YEAR_IN_REVIEW.CONVERSATIONS.COMPARISON.10000_PLUS");
}

function busiestComparison(count: number): string {
  if (count < 5) return t("YEAR_IN_REVIEW.BUSIEST_DAY.COMPARISON.0_5");
  if (count < 10) return t("YEAR_IN_REVIEW.BUSIEST_DAY.COMPARISON.5_10");
  if (count < 25) return t("YEAR_IN_REVIEW.BUSIEST_DAY.COMPARISON.10_25");
  if (count < 50) return t("YEAR_IN_REVIEW.BUSIEST_DAY.COMPARISON.25_50");
  if (count < 100) return t("YEAR_IN_REVIEW.BUSIEST_DAY.COMPARISON.50_100");
  if (count < 500) return t("YEAR_IN_REVIEW.BUSIEST_DAY.COMPARISON.100_500");
  return t("YEAR_IN_REVIEW.BUSIEST_DAY.COMPARISON.500_PLUS");
}

function personalityCopy(avgSeconds: number): string {
  const formatted = formatTime(avgSeconds);
  if (avgSeconds <= 120) return t("YEAR_IN_REVIEW.PERSONALITY.MESSAGES.SWIFT_HELPER", { time: formatted });
  if (avgSeconds <= 600) return t("YEAR_IN_REVIEW.PERSONALITY.MESSAGES.QUICK_RESPONDER", { time: formatted });
  if (avgSeconds <= 3600) return t("YEAR_IN_REVIEW.PERSONALITY.MESSAGES.STEADY_SUPPORT", { time: formatted });
  return t("YEAR_IN_REVIEW.PERSONALITY.MESSAGES.THOUGHTFUL_ADVISOR", { time: formatted });
}

export default function YearInReviewReport() {
  const { accountId } = useOutletContext<ShellCtx>();
  const currentYear = new Date().getFullYear();
  const years = [currentYear, currentYear - 1, currentYear - 2];
  const [year, setYear] = useState<number>(currentYear);
  const [slide, setSlide] = useState(0);
  const [data, setData] = useState<YearInReviewData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    api<YearInReviewData>(`/api/v2/accounts/${accountId}/year_in_review?year=${year}`)
      .then((res) => {
        if (active) setData(res);
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : t("YEAR_IN_REVIEW.ERROR"));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [accountId, year]);

  async function shareSummary() {
    if (!data) return;
    const summary = [
      t("YEAR_IN_REVIEW.SHARE_MODAL.SHARE_TITLE", { year: data.year }),
      `${t("YEAR_IN_REVIEW.CONVERSATIONS.TITLE")} ${data.total_conversations} ${t("YEAR_IN_REVIEW.CONVERSATIONS.SUBTITLE")}`,
      data.busiest_day ? `${t("YEAR_IN_REVIEW.BUSIEST_DAY.TITLE")} ${data.busiest_day.date} (${data.busiest_day.count})` : "",
      personalityCopy(data.support_personality.avg_response_time_seconds),
    ]
      .filter(Boolean)
      .join("\n");
    try {
      await navigator.clipboard.writeText(summary);
      setCopied(true);
      setTimeout(() => setCopied(false), 2500);
    } catch {
      setCopied(false);
    }
  }

  const cards = data
    ? [
        {
          id: "conversations",
          icon: MessageSquare,
          eyebrow: t("YEAR_IN_REVIEW.CONVERSATIONS.TITLE"),
          headline: `${data.total_conversations.toLocaleString()} ${t("YEAR_IN_REVIEW.CONVERSATIONS.SUBTITLE")}`,
          body: data.total_conversations === 0 ? t("YEAR_IN_REVIEW.CONVERSATIONS.FALLBACK") : volumeComparison(data.total_conversations),
          accent: "from-blue-600 to-indigo-700",
        },
        {
          id: "busiest",
          icon: Flame,
          eyebrow: t("YEAR_IN_REVIEW.BUSIEST_DAY.TITLE"),
          headline: data.busiest_day ? data.busiest_day.date : "No traffic recorded yet",
          body: data.busiest_day
            ? `${t("YEAR_IN_REVIEW.BUSIEST_DAY.MESSAGE", { count: data.busiest_day.count })} ${busiestComparison(data.busiest_day.count)}`
            : t("YEAR_IN_REVIEW.CONVERSATIONS.FALLBACK"),
          accent: "from-amber-500 to-rose-600",
        },
        {
          id: "personality",
          icon: Sparkles,
          eyebrow: t("YEAR_IN_REVIEW.PERSONALITY.TITLE"),
          headline: formatTime(data.support_personality.avg_response_time_seconds),
          body: personalityCopy(data.support_personality.avg_response_time_seconds),
          accent: "from-teal-500 to-emerald-700",
        },
        {
          id: "thankyou",
          icon: Calendar,
          eyebrow: `${data.year}`,
          headline: t("YEAR_IN_REVIEW.THANK_YOU.TITLE", { year: data.year }),
          body: t("YEAR_IN_REVIEW.THANK_YOU.MESSAGE", { nextYear: data.year + 1 }),
          accent: "from-violet-600 to-fuchsia-700",
        },
      ]
    : [];

  const activeCard = cards[slide] ?? cards[0];

  return (
    <div className="h-full overflow-auto bg-n-background p-6 text-n-slate-12">
      <div className="mx-auto max-w-4xl space-y-6">
        <header className="flex flex-wrap items-center justify-between gap-4 border-b border-n-weak pb-4">
          <div className="flex items-center gap-3">
            <Link
              to={`/accounts/${accountId}/reports`}
              className="inline-flex items-center gap-1.5 text-sm text-n-slate-11 hover:text-n-slate-12"
            >
              <ArrowLeft className="size-4" />
              <span>{t("REPORT.HEADER")}</span>
            </Link>
            <span className="text-n-slate-8">/</span>
            <h1 className="font-inter-display text-xl font-semibold">
              {t("YEAR_IN_REVIEW.TITLE")}
            </h1>
          </div>
          <div className="flex items-center gap-3">
            <select
              aria-label="Year"
              value={year}
              onChange={(e) => {
                setYear(Number(e.target.value));
                setSlide(0);
              }}
              className="h-9 rounded-lg border border-n-weak bg-n-solid-1 px-3 text-sm text-n-slate-12"
            >
              {years.map((y) => (
                <option key={y} value={y}>
                  {y}
                </option>
              ))}
            </select>
            {data && (
              <Button sm outline slate onClick={() => void shareSummary()} label={copied ? "Copied summary" : t("YEAR_IN_REVIEW.NAVIGATION.SHARE")}>
                <Share2 className="size-3.5" />
              </Button>
            )}
          </div>
        </header>

        {error && (
          <p role="alert" className="rounded-xl border border-n-ruby-7 bg-n-ruby-2 p-4 text-sm text-n-ruby-11">
            {error}
          </p>
        )}

        {loading ? (
          <div className="rounded-2xl border border-n-weak bg-n-solid-2 p-10 text-center text-sm text-n-slate-11">
            {t("YEAR_IN_REVIEW.LOADING")}
          </div>
        ) : activeCard && data ? (
          <div className="space-y-6">
            <div className={`rounded-2xl bg-gradient-to-br ${activeCard.accent} p-8 text-white shadow-lg`}>
              <div className="mb-4 inline-flex size-11 items-center justify-center rounded-xl bg-white/15">
                <activeCard.icon className="size-6" />
              </div>
              <p className="text-sm font-medium text-white/80">{activeCard.eyebrow}</p>
              <h2 className="mt-2 font-inter-display text-3xl font-bold">{activeCard.headline}</h2>
              <p className="mt-4 max-w-2xl text-base leading-relaxed text-white/90">{activeCard.body}</p>
            </div>

            <div className="flex items-center justify-between">
              <Button
                sm
                slate
                outline
                disabled={slide === 0}
                label={t("YEAR_IN_REVIEW.NAVIGATION.PREVIOUS")}
                onClick={() => setSlide((s) => Math.max(0, s - 1))}
              />
              <div className="flex gap-2">
                {cards.map((c, idx) => (
                  <button
                    key={c.id}
                    type="button"
                    aria-label={c.id}
                    onClick={() => setSlide(idx)}
                    className={`h-2 rounded-full transition-all ${
                      idx === slide ? "w-6 bg-n-brand" : "w-2 bg-n-weak hover:bg-n-slate-8"
                    }`}
                  />
                ))}
              </div>
              <Button
                sm
                solid
                blue
                disabled={slide >= cards.length - 1}
                label={t("YEAR_IN_REVIEW.NAVIGATION.NEXT")}
                onClick={() => setSlide((s) => Math.min(cards.length - 1, s + 1))}
              />
            </div>

            <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
              <div className="rounded-xl border border-n-weak bg-n-solid-1 p-4">
                <div className="text-xs text-n-slate-11">{t("YEAR_IN_REVIEW.CONVERSATIONS.SUBTITLE")}</div>
                <div className="mt-1 text-2xl font-semibold">{data.total_conversations.toLocaleString()}</div>
              </div>
              <div className="rounded-xl border border-n-weak bg-n-solid-1 p-4">
                <div className="text-xs text-n-slate-11">{t("YEAR_IN_REVIEW.BUSIEST_DAY.TITLE")}</div>
                <div className="mt-1 text-2xl font-semibold">
                  {data.busiest_day ? `${data.busiest_day.date} (${data.busiest_day.count})` : "—"}
                </div>
              </div>
              <div className="rounded-xl border border-n-weak bg-n-solid-1 p-4">
                <div className="text-xs text-n-slate-11">{t("YEAR_IN_REVIEW.PERSONALITY.TITLE")}</div>
                <div className="mt-1 text-2xl font-semibold">{formatTime(data.support_personality.avg_response_time_seconds)}</div>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
