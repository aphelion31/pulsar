// Interactive SVG BarChart matching Chatwoot @chatwoot/viz BarChart.
import { useState } from "react";

export type BarSeries = {
  id?: string;
  label: string;
  color?: string;
  data: number[];
};

export function BarChart({
  data,
  height = 280,
  barRadius = 6,
  yStepSize,
  ariaLabel = "Chart",
  className = "",
}: {
  data: { categories: string[]; series: BarSeries[] };
  height?: number;
  maxBarWidth?: number;
  barRadius?: number;
  yStepSize?: number;
  timeseries?: boolean;
  ariaLabel?: string;
  className?: string;
}) {
  const [hovered, setHovered] = useState<number | null>(null);
  const categories = data.categories ?? [];
  const primary = data.series[0] ?? { label: "Value", color: "#2781F6", data: [] };
  const values = primary.data ?? [];
  const color = primary.color || "#2781F6";

  if (!categories.length || !values.length) {
    return (
      <div
        className={`flex items-center justify-center rounded-lg border border-dashed border-n-weak text-sm text-n-slate-10 ${className}`}
        style={{ height }}
      >
        No data available
      </div>
    );
  }

  const rawMax = Math.max(1, ...values);
  const step = yStepSize && yStepSize > 0 ? yStepSize : Math.max(1, Math.ceil(rawMax / 4));
  const yMax = Math.max(step * 4, Math.ceil(rawMax / step) * step);
  const ticks = [0, step, step * 2, step * 3, yMax];

  // Auto-thin X axis labels so 30-day charts remain legible without ellipsis truncation.
  const labelStride = Math.max(1, Math.ceil(categories.length / 7));
  const xTicks = categories.filter(
    (_, i) => i % labelStride === 0 || (i === categories.length - 1 && (categories.length - 1) % labelStride >= Math.ceil(labelStride / 2))
  );

  return (
    <div className={`relative w-full select-none ${className}`} aria-label={ariaLabel} role="img">
      <div className="flex gap-3" style={{ height }}>
        {/* Y-Axis Ticks */}
        <div className="flex shrink-0 flex-col-reverse justify-between pb-6 text-right font-mono text-[11px] text-n-slate-10">
          {ticks.map((t) => (
            <span key={t} className="leading-none">
              {t.toLocaleString()}
            </span>
          ))}
        </div>

        {/* Plot Area */}
        <div className="relative flex min-w-0 flex-1 flex-col">
          <div className="relative flex-1">
            {/* Horizontal dashed gridlines */}
            <div className="pointer-events-none absolute inset-0 flex flex-col justify-between">
              {ticks
                .slice()
                .reverse()
                .map((t) => (
                  <div
                    key={t}
                    className={`w-full border-t ${
                      t === 0 ? "border-n-weak" : "border-dashed border-n-weak/80"
                    }`}
                  />
                ))}
            </div>

            {/* Bars */}
            <div className="relative z-10 flex h-full items-end gap-1 sm:gap-1.5 px-1">
              {values.map((val, i) => {
                const pct = yMax > 0 ? (val / yMax) * 100 : 0;
                const isHover = hovered === i;
                return (
                  <div
                    key={`${categories[i]}-${i}`}
                    className="group relative flex h-full flex-1 flex-col items-center justify-end"
                    onMouseEnter={() => setHovered(i)}
                    onMouseLeave={() => setHovered(null)}
                  >
                    {isHover && (
                      <div className="pointer-events-none absolute -top-8 z-30 whitespace-nowrap rounded-lg border border-n-weak bg-n-solid-1 px-2 py-1 text-[11px] font-medium text-n-slate-12 shadow-lg">
                        <span className="text-n-slate-10">{categories[i]}: </span>
                        <span className="font-mono font-semibold">{val.toLocaleString()}</span>
                      </div>
                    )}
                    <div
                      className="w-full max-w-[28px] transition-all duration-150"
                      style={{
                        height: val > 0 ? `${Math.max(3, pct)}%` : "2px",
                        backgroundColor: val > 0 ? color : "rgba(var(--slate-6), 0.35)",
                        borderTopLeftRadius: `${barRadius}px`,
                        borderTopRightRadius: `${barRadius}px`,
                        opacity: hovered !== null && !isHover ? 0.55 : 1,
                      }}
                    />
                  </div>
                );
              })}
            </div>
          </div>

          {/* X-Axis Categories */}
          <div className="mt-2 flex h-4 items-center justify-between px-1 font-mono text-[11px] text-n-slate-10">
            {xTicks.map((cat, i) => (
              <span key={`${cat}-${i}`} className="whitespace-nowrap">
                {cat}
              </span>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
