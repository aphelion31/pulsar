import { useEffect, useState } from "react";
import { api } from "~/lib/api";

export default function Bot({ accountId }: { accountId: number }) {
  const [metrics, setMetrics] = useState<Record<string, number>>();
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    api<Record<string, number>>(`/api/v2/accounts/${accountId}/reports/bot_metrics`).then(data => { if (active) setMetrics(data); }).catch((err: unknown) => { if (active) setError(err instanceof Error ? err.message : "Failed to load bot reports"); });
    return () => { active = false; };
  }, [accountId]);
  return <div className="h-full overflow-auto p-6"><h1 className="mb-6 text-xl font-semibold">Bot Reports</h1>{error && <p role="alert">{error}</p>}<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">{metrics && Object.entries(metrics).map(([key, value]) => <section key={key} className="rounded-lg border border-slate-200 p-5"><h2 className="text-sm capitalize">{key.replaceAll("_", " ")}</h2><p className="mt-3 text-2xl font-semibold">{value}{key.endsWith("rate") ? "%" : ""}</p></section>)}</div></div>;
}
