import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MarketGroup, MarketGroupOutcome } from "../../types";

// ── Palette — 14 perceptually distinct colours ───────────────────────
// Ordered so adjacent ranks don't clash.
export const OUTCOME_COLORS = [
  "#34d399", // emerald-400
  "#60a5fa", // blue-400
  "#f472b6", // pink-400
  "#fb923c", // orange-400
  "#a78bfa", // violet-400
  "#fbbf24", // amber-400
  "#38bdf8", // sky-400
  "#f87171", // red-400
  "#4ade80", // green-400
  "#c084fc", // purple-400
  "#2dd4bf", // teal-400
  "#facc15", // yellow-400
  "#818cf8", // indigo-400
  "#e879f9", // fuchsia-400
];

// ── Deterministic seeded RNG (same pattern as mock-price-history) ────
function hashCode(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}
function seededRand(seed: number): number {
  const x = Math.sin(seed + 1) * 10000;
  return x - Math.floor(x);
}

// ── Mock price series for one outcome ───────────────────────────────
function generateOutcomeSeries(
  outcome: MarketGroupOutcome,
  pointCount = 120,
): number[] {
  const seed = hashCode(outcome.id);
  const end = outcome.yesPrice;

  // Pick one of 4 simple shapes deterministically
  const shapeIdx = seed % 4;
  const noise = (i: number, amp: number) =>
    (seededRand(seed + i * 7) - 0.5) * 2 * amp;

  return Array.from({ length: pointCount }, (_, i) => {
    const t = i / (pointCount - 1);
    let base: number;
    switch (shapeIdx) {
      case 0: {
        // steady drift toward end
        const start = Math.max(0.02, end - 0.18 - seededRand(seed) * 0.1);
        base = start + (end - start) * t;
        break;
      }
      case 1: {
        // late surge
        const flat = Math.max(0.02, end - 0.15);
        base = t < 0.65 ? flat : flat + (end - flat) * ((t - 0.65) / 0.35);
        break;
      }
      case 2: {
        // slight peak then settle
        const peak = Math.min(0.95, end + 0.12 + seededRand(seed) * 0.08);
        const start = end + (seededRand(seed + 1) - 0.5) * 0.05;
        base =
          t < 0.4
            ? start + (peak - start) * (t / 0.4)
            : peak + (end - peak) * ((t - 0.4) / 0.6);
        break;
      }
      default: {
        // volatile walk
        const start2 = 0.25 + seededRand(seed) * 0.3;
        base = start2 + (end - start2) * t;
        break;
      }
    }
    return Math.max(0.005, Math.min(0.995, base + noise(i, 0.018)));
  });
}

// ── SVG polyline path from normalised values ─────────────────────────
function buildPath(
  series: number[],
  W: number,
  H: number,
  padT: number,
  padB: number,
): string {
  const drawH = H - padT - padB;
  return series
    .map((v, i) => {
      const x = (i / (series.length - 1)) * W;
      const y = padT + (1 - v) * drawH;
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
}

// ── Hover tooltip ────────────────────────────────────────────────────
type TooltipData = {
  x: number;
  svgX: number;
  items: { name: string; color: string; pct: number }[];
};

// ── Legend item ──────────────────────────────────────────────────────
function LegendItem({
  color,
  name,
  pct,
  dimmed,
  onClick,
}: {
  color: string;
  name: string;
  pct: number;
  dimmed: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-1.5 rounded px-2 py-1 text-left transition ${dimmed ? "opacity-30" : "opacity-100 hover:bg-slate-800/60"}`}
    >
      <span
        className="h-2 w-2 shrink-0 rounded-full"
        style={{ backgroundColor: color }}
      />
      <span className="truncate text-[11px] text-slate-300">{name}</span>
      <span className="ml-auto shrink-0 text-[11px] font-medium text-slate-400">
        {pct}%
      </span>
    </button>
  );
}

// ── Main chart component ─────────────────────────────────────────────
export default function GroupChart({
  group,
  highlightedOutcomeId,
}: {
  group: MarketGroup;
  highlightedOutcomeId: string | null;
}) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [dims, setDims] = useState({ w: 600, h: 220 });
  const [tooltip, setTooltip] = useState<TooltipData | null>(null);
  // Which outcomes are toggled OFF in the legend
  const [hidden, setHidden] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!svgRef.current) return;
    const ro = new ResizeObserver((entries) => {
      const { width } = entries[0].contentRect;
      setDims({ w: width, h: Math.round(width * 0.36) });
    });
    ro.observe(svgRef.current);
    return () => ro.disconnect();
  }, []);

  // Sorted by current probability descending (same order as rows)
  const rankedOutcomes = useMemo(
    () => [...group.outcomes].sort((a, b) => b.yesPrice - a.yesPrice),
    [group.outcomes],
  );

  // Only show top-8 in chart to avoid clutter; rest grouped as "Other"
  const TOP_N = 8;
  const chartOutcomes = rankedOutcomes.slice(0, TOP_N);

  const seriesMap = useMemo(() => {
    const m = new Map<string, number[]>();
    for (const o of chartOutcomes) {
      m.set(o.id, generateOutcomeSeries(o));
    }
    return m;
  }, [chartOutcomes]);

  const PAD_T = 12;
  const PAD_B = 20;

  const toggleHidden = useCallback((id: string) => {
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<SVGSVGElement>) => {
      const rect = svgRef.current?.getBoundingClientRect();
      if (!rect) return;
      const relX = e.clientX - rect.left;
      const fraction = relX / rect.width;
      const idx = Math.round(fraction * 119);
      const clamped = Math.max(0, Math.min(119, idx));

      const items = chartOutcomes
        .filter((o) => !hidden.has(o.id))
        .map((o, i) => {
          const series = seriesMap.get(o.id) ?? [];
          const val = series[clamped] ?? o.yesPrice;
          return {
            name: o.name,
            color: OUTCOME_COLORS[i % OUTCOME_COLORS.length],
            pct: Math.round(val * 100),
          };
        })
        .sort((a, b) => b.pct - a.pct);

      setTooltip({ x: relX, svgX: fraction * dims.w, items });
    },
    [chartOutcomes, hidden, seriesMap, dims.w],
  );

  const handleMouseLeave = useCallback(() => setTooltip(null), []);

  const { w, h } = dims;

  // Y-axis grid lines at 25% intervals
  const yGridLines = [0.25, 0.5, 0.75];

  return (
    <div className="space-y-3">
      {/* SVG chart */}
      <div className="relative overflow-hidden rounded-xl border border-slate-800 bg-slate-950/60">
        <svg
          ref={svgRef}
          aria-label="Outcome probability chart"
          role="img"
          viewBox={`0 0 ${w} ${h}`}
          className="w-full"
          onMouseMove={handleMouseMove}
          onMouseLeave={handleMouseLeave}
          style={{ display: "block" }}
        >
          {/* Y-axis grid */}
          {yGridLines.map((v) => {
            const y = PAD_T + (1 - v) * (h - PAD_T - PAD_B);
            return (
              <g key={v}>
                <line
                  x1={0}
                  y1={y}
                  x2={w}
                  y2={y}
                  stroke="#1e293b"
                  strokeWidth="1"
                />
                <text
                  x={4}
                  y={y - 3}
                  fill="#475569"
                  fontSize="8"
                  fontFamily="monospace"
                >
                  {Math.round(v * 100)}%
                </text>
              </g>
            );
          })}

          {/* Lines — dimmed-out outcomes drawn first, then normal */}
          {chartOutcomes.map((outcome, i) => {
            const series = seriesMap.get(outcome.id);
            if (!series) return null;
            const color = OUTCOME_COLORS[i % OUTCOME_COLORS.length];
            const isHidden = hidden.has(outcome.id);
            const isHighlighted = highlightedOutcomeId === outcome.id;
            const pts = buildPath(series, w, h, PAD_T, PAD_B);
            const opacity = isHidden
              ? 0
              : isHighlighted || !highlightedOutcomeId
                ? 1
                : 0.25;
            const strokeW = isHighlighted ? 2.5 : 1.5;

            return (
              <polyline
                key={outcome.id}
                points={pts}
                stroke={color}
                strokeWidth={strokeW}
                strokeLinecap="round"
                strokeLinejoin="round"
                fill="none"
                style={{ opacity, transition: "opacity 0.15s" }}
              />
            );
          })}

          {/* Hover crosshair */}
          {tooltip && (
            <line
              x1={tooltip.svgX}
              y1={PAD_T}
              x2={tooltip.svgX}
              y2={h - PAD_B}
              stroke="#64748b"
              strokeWidth="1"
              strokeDasharray="3 3"
            />
          )}
        </svg>

        {/* Tooltip */}
        {tooltip && (
          <div
            className="pointer-events-none absolute top-2 z-10 w-44 rounded-lg border border-slate-700 bg-slate-900/95 p-2 text-xs shadow-lg backdrop-blur-sm"
            style={{
              left: tooltip.x > dims.w * 0.6 ? tooltip.x - 184 : tooltip.x + 12,
            }}
          >
            {tooltip.items.slice(0, 5).map((item) => (
              <div
                key={item.name}
                className="flex items-center justify-between gap-2 py-0.5"
              >
                <span className="flex items-center gap-1.5 min-w-0">
                  <span
                    className="h-1.5 w-1.5 shrink-0 rounded-full"
                    style={{ backgroundColor: item.color }}
                  />
                  <span className="truncate text-slate-300">{item.name}</span>
                </span>
                <span className="shrink-0 font-medium text-slate-200">
                  {item.pct}%
                </span>
              </div>
            ))}
            {tooltip.items.length > 5 && (
              <p className="mt-1 text-slate-600">
                +{tooltip.items.length - 5} more
              </p>
            )}
          </div>
        )}
      </div>

      {/* Legend — two-column grid */}
      <div className="grid grid-cols-2 gap-x-2 gap-y-0.5 sm:grid-cols-3 lg:grid-cols-4">
        {chartOutcomes.map((outcome, i) => {
          const color = OUTCOME_COLORS[i % OUTCOME_COLORS.length];
          const pct = Math.round(outcome.yesPrice * 100);
          return (
            <LegendItem
              key={outcome.id}
              color={color}
              name={outcome.name}
              pct={pct}
              dimmed={hidden.has(outcome.id)}
              onClick={() => toggleHidden(outcome.id)}
            />
          );
        })}
        {rankedOutcomes.length > TOP_N && (
          <div className="flex items-center gap-1.5 px-2 py-1">
            <span className="h-2 w-2 shrink-0 rounded-full bg-slate-700" />
            <span className="text-[11px] text-slate-600">
              +{rankedOutcomes.length - TOP_N} others
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
