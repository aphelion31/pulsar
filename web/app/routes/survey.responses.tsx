import { useEffect, useState } from "react";
import { useParams } from "react-router";
import { Button, ErrorText } from "~/components/ui";

const RATINGS = [
  { value: 1, label: "1 - Awful" },
  { value: 2, label: "2 - Poor" },
  { value: 3, label: "3 - Average" },
  { value: 4, label: "4 - Good" },
  { value: 5, label: "5 - Excellent" },
];

export default function SurveyResponseRoute() {
  const { id } = useParams();
  const endpoint = `/public/api/v1/csat_survey/${encodeURIComponent(id ?? "")}`;
  const [open, setOpen] = useState(false);
  const [rating, setRating] = useState<number>(5);
  const [feedback, setFeedback] = useState("");
  const [status, setStatus] = useState("Loading survey...");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    setOpen(false);
    setError("");
    fetch(endpoint, { signal: controller.signal })
      .then(async (r) => {
        if (!r.ok) throw new Error("Survey not found");
        return r.json();
      })
      .then((d) => {
        if (controller.signal.aborted) return;
        const canRate = d.open !== false && d.rating == null;
        setOpen(Boolean(d.open ?? true));
        if (d.rating != null) setRating(Number(d.rating));
        setFeedback(d.feedback_message ?? "");
        setStatus(canRate ? "" : d.rating != null ? "Thank you for your feedback!" : "This survey is closed.");
      })
      .catch((e) => {
        if (!controller.signal.aborted) {
          setError(e instanceof Error ? e.message : "Survey unavailable");
          setStatus("");
        }
      });
    return () => {
      controller.abort();
    };
  }, [endpoint]);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const r = await fetch(endpoint, {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          message: {
            submitted_values: {
              csat_survey_response: {
                rating,
                feedback_message: feedback,
              },
            },
          },
        }),
      });
      if (!r.ok) {
        const body = await r.json().catch(() => ({}));
        throw new Error(body.error || "Could not save rating");
      }
      setStatus("Thank you! Your rating has been saved.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save rating");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-dvh place-items-center bg-bg px-6 py-12">
      <div className="w-full max-w-md rounded-xl border border-line bg-surface p-6">
        <h1 className="text-xl font-semibold text-ink">Rate your conversation</h1>
        <p className="mt-1 text-sm text-ink-2">Please let us know how we did.</p>

        {open && (
          <form onSubmit={submit} className="mt-6 grid gap-4">
            <fieldset className="grid gap-2">
              <legend className="text-sm font-medium text-ink-2">Rating</legend>
              <div className="grid gap-1.5">
                {RATINGS.map((opt) => (
                  <label
                    key={opt.value}
                    className={`flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-2 text-sm ${
                      rating === opt.value ? "border-accent bg-accent-soft text-ink" : "border-line text-ink-2"
                    }`}
                  >
                    <input
                      type="radio"
                      name="rating"
                      value={opt.value}
                      checked={rating === opt.value}
                      onChange={() => setRating(opt.value)}
                    />
                    {opt.label}
                  </label>
                ))}
              </div>
            </fieldset>

            <label className="grid gap-1.5 text-sm text-ink-2">
              Feedback (optional)
              <textarea
                rows={3}
                maxLength={4000}
                value={feedback}
                onChange={(e) => setFeedback(e.target.value)}
                placeholder="Tell us more about your experience..."
                className="w-full rounded-lg border border-line-strong bg-surface p-3 text-sm text-ink outline-none focus:border-accent"
              />
            </label>

            <ErrorText>{error}</ErrorText>
            <Button type="submit" disabled={busy} className="h-9 justify-center">
              {busy ? "Submitting..." : "Submit rating"}
            </Button>
          </form>
        )}

        {!open && <ErrorText>{error}</ErrorText>}
        {status && (
          <p role="status" className="mt-4 text-sm font-medium text-ink">
            {status}
          </p>
        )}
      </div>
    </main>
  );
}
