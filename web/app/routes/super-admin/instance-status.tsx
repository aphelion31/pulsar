import { useEffect, useState } from "react";
import { api } from "~/lib/api";

export default function SuperAdminInstanceStatusRoute() {
  const [metrics, setMetrics] = useState<Record<string, string>>({});
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    api<{ metrics: Record<string, string> }>("/super_admin/api/instance_status")
      .then((res) => {
        if (active) setMetrics(res.metrics ?? {});
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : "Failed to load status");
      });
    return () => {
      active = false;
    };
  }, []);

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <h1 className="text-xl font-semibold text-ink">Instance Status</h1>
      {error && <p className="text-sm text-bad">{error}</p>}
      <div className="overflow-hidden rounded-xl border border-line bg-surface">
        <table className="w-full text-left text-sm">
          <tbody className="divide-y divide-line">
            {Object.entries(metrics).map(([k, v]) => (
              <tr key={k}>
                <td className="w-1/3 px-4 py-3 font-medium text-ink">{k}</td>
                <td className="px-4 py-3 font-mono text-xs text-ink-2">{String(v)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
