import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MarketGroup, MarketGroupOutcome } from "../../types";

// ── Palette — 14 perceptually distinct colours ───────────────────────
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

// ── Timescale config ─────────────────────────────────────────────────
type Timescale = "1h" | "4h" | "1d" | "3d" | "7d" | "1M" | "ALL";

const TIMESCALES: Timescale[] = ["1h", "4h", "1d", "3d", "7d", "1M", "ALL"];

// Blocks per timescale window (1 block ≈ 1 min on Liquid)
const SCALE_BLOCKS: Record<Timescale, number> = {
  "1h": 60,
  "4h": 240,
  "1d": 1440,
  "3d": 4320,
  "7d": 10080,
  "1M": 43200,
  ALL: 64800,
};

// How many data points to slice from the full 120-pt series
const SCALE_POINTS: Record<Timescale, number> = {
  "1h": 8,
  "4h": 20,
  "1d": 40,
  "3d": 60,
  "7d": 80,
  "1M": 100,
  ALL: 120,
};

// X-axis label count per timescale
function xLabelsForScale(scale: Timescale, currentHeight: number): string[] {
  const blocks = SCALE_BLOCKS[scale];
  const startBlock = currentHeight - blocks;
  const count = 5;
  return Array.from({ length: count }, (_, i) => {
    const block = startBlock + (blocks * i) / (count - 1);
    const minutesFromNow = (block - currentHeight) * 1; // 1 block ≈ 1 min
    if (minutesFromNow >= -1) return "Now";
    const absMin = Math.abs(minutesFromNow);
    if (scale === "1h" || scale === "4h") {
      const h = Math.floor(absMin / 60);
      const m = Math.round(absMin % 60);
      return h > 0 ? `${h}h${m > 0 ? `${m}m` : ""} ago` : `${m}m ago`;
    }
    if (scale === "1d" || scale === "3d") {
      const h = Math.round(absMin / 60);
      return `${h}h ago`;
    }
    const days = Math.round(absMin / 1440);
    return `${days}d ago`;
  });
}

// ── Deterministic seeded RNG ─────────────────────────────────────────
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

// ── Mock price series for one outcome (120 full points) ──────────────
function generateOutcomeSeries(outcome: MarketGroupOutcome): number[] {
  const POINT_COUNT = 120;
  const seed = hashCode(outcome.id);
  const end = outcome.yesPrice;
  const shapeIdx = seed % 4;
  const noise = (i: number, amp: number) =>
    (seededRand(seed + i * 7) - 0.5) * 2 * amp;

  return Array.from({ length: POINT_COUNT }, (_, i) => {
    const t = i / (POINT_COUNT - 1);
    let base: number;
    switch (shapeIdx) {
      case 0: {
        const start = Math.max(0.02, end - 0.18 - seededRand(seed) * 0.1);
        base = start + (end - start) * t;
        break;
      }
      case 1: {
        const flat = Math.max(0.02, end - 0.15);
        base = t < 0.65 ? flat : flat + (end - flat) * ((t - 0.65) / 0.35);
        break;
      }
      case 2: {
        const peak = Math.min(0.95, end + 0.12 + seededRand(seed) * 0.08);
        const start = end + (seededRand(seed + 1) - 0.5) * 0.05;
        base =
          t < 0.4
            ? start + (peak - start) * (t / 0.4)
            : peak + (end - peak) * ((t - 0.4) / 0.6);
        break;
      }
      default: {
        const start2 = 0.25 + seededRand(seed) * 0.3;
        base = start2 + (end - start2) * t;
        break;
      }
    }
    return Math.max(0.005, Math.min(0.995, base + noise(i, 0.018)));
  });
}

// ── SVG polyline points from a series slice ──────────────────────────
function buildPoints(
  series: number[],
  W: number,
  H: number,
  padL: number,
  padR: number,
  padT: number,
  padB: number,
): string {
  const drawW = W - padL - padR;
  const drawH = H - padT - padB;
  return series
    .map((v, i) => {
      const x = padL + (i / (series.length - 1)) * drawW;
      const y = padT + (1 - v) * drawH;
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
}

// ── Tooltip data ─────────────────────────────────────────────────────
type TooltipData = {
  clientX: number;
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
      className={`flex items-center gap-1.5 rounded px-2 py-1 text-left transition ${
        dimmed ? "opacity-30" : "opacity-100 hover:bg-slate-800/60"
      }`}
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

// ── Main GroupChart ──────────────────────────────────────────────────
export default function GroupChart({
  group,
  highlightedOutcomeId,
}: {
  group: MarketGroup;
  highlightedOutcomeId: string | null;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [dims, setDims] = useState({ w: 600, h: 216 });
  const [timescale, setTimescale] = useState<Timescale>("7d");
  const [tooltip, setTooltip] = useState<TooltipData | null>(null);
  const [hidden, setHidden] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!containerRef.current) return;
    const ro = new ResizeObserver((entries) => {
      const { width } = entries[0].contentRect;
      setDims({ w: width, h: Math.round(width * 0.34) });
    });
    ro.observe(containerRef.current);
    return () => ro.disconnect();
  }, []);

  // Top 8 outcomes by probability
  const TOP_N = 8;
  const rankedOutcomes = useMemo(
    () => [...group.outcomes].sort((a, b) => b.yesPrice - a.yesPrice),
    [group.outcomes],
  );
  const chartOutcomes = rankedOutcomes.slice(0, TOP_N);

  // Full 120-pt series per outcome (stable across timescale changes)
  const fullSeriesMap = useMemo(() => {
    const m = new Map<string, number[]>();
    for (const o of chartOutcomes) m.set(o.id, generateOutcomeSeries(o));
    return m;
  }, [chartOutcomes]);

  // Slice to the current timescale window
  const visiblePoints = SCALE_POINTS[timescale];
  const seriesMap = useMemo(() => {
    const m = new Map<string, number[]>();
    for (const o of chartOutcomes) {
      const full = fullSeriesMap.get(o.id) ?? [];
      m.set(o.id, full.slice(full.length - visiblePoints));
    }
    return m;
  }, [chartOutcomes, fullSeriesMap, visiblePoints]);

  // Layout constants (SVG-space)
  const { w, h } = dims;
  const PAD_L = 4;
  const PAD_R = 30; // space for y-axis labels on the right
  const PAD_T = 6;
  const PAD_B = 20; // space for x-axis labels at bottom

  const yFromValue = (v: number) => PAD_T + (1 - v) * (h - PAD_T - PAD_B);
  const plotW = w - PAD_L - PAD_R;

  const toggleHidden = useCallback((id: string) => {
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const relX = e.clientX - rect.left;
      const fraction = Math.max(0, Math.min(1, (relX - PAD_L) / plotW));
      const idx = Math.round(fraction * (visiblePoints - 1));

      const items = chartOutcomes
        .filter((o) => !hidden.has(o.id))
        .map((o, i) => {
          const series = seriesMap.get(o.id) ?? [];
          const val = series[idx] ?? o.yesPrice;
          return {
            name: o.name,
            color: OUTCOME_COLORS[i % OUTCOME_COLORS.length],
            pct: Math.round(val * 100),
          };
        })
        .sort((a, b) => b.pct - a.pct);

      setTooltip({
        clientX: relX,
        svgX: PAD_L + fraction * plotW,
        items,
      });
    },
    [chartOutcomes, hidden, seriesMap, plotW, visiblePoints],
  );

  const handleMouseLeave = useCallback(() => setTooltip(null), []);

  // Y-axis guide levels
  const yLevels = [0, 0.25, 0.5, 0.75, 1];

  // X-axis labels
  const xLabels = xLabelsForScale(timescale, group.currentHeight);

  return (
    <div className="space-y-2">
      {/* Chart container */}
      <div
        ref={containerRef}
        className="relative rounded-xl border border-slate-800 bg-slate-950/60"
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
      >
        <svg
          aria-label="Outcome probability chart"
          role="img"
          viewBox={`0 0 ${w} ${h}`}
          className="w-full"
          style={{ display: "block" }}
        >
          {/* Y-axis grid lines + labels */}
          {yLevels.map((v) => {
            const y = yFromValue(v);
            return (
              <g key={v}>
                <line
                  x1={PAD_L}
                  y1={y}
                  x2={w - PAD_R}
                  y2={y}
                  stroke="#64748b"
                  strokeOpacity="0.2"
                  strokeWidth="0.6"
                  strokeDasharray={v === 0 || v === 1 ? undefined : "2 4"}
                />
                {/* Y label on right edge */}
                <text
                  x={w - PAD_R + 3}
                  y={y}
                  fill="#475569"
                  fontSize="7"
                  dominantBaseline="middle"
                  textAnchor="start"
                  fontFamily="monospace"
                >
                  {`${Math.round(v * 100)}%`}
                </text>
              </g>
            );
          })}

          {/* X-axis labels */}
          {xLabels.map((label, i) => {
            const x = PAD_L + (i / (xLabels.length - 1)) * plotW;
            const anchor =
              i === 0 ? "start" : i === xLabels.length - 1 ? "end" : "middle";
            return (
              <text
                key={`xl-${i}`}
                x={x}
                y={h - 3}
                fill="#475569"
                fontSize="7"
                dominantBaseline="auto"
                textAnchor={anchor}
                fontFamily="monospace"
              >
                {label}
              </text>
            );
          })}

          {/* Series lines */}
          {chartOutcomes.map((outcome, i) => {
            const series = seriesMap.get(outcome.id);
            if (!series || series.length < 2) return null;
            const color = OUTCOME_COLORS[i % OUTCOME_COLORS.length];
            const isHidden = hidden.has(outcome.id);
            const isHighlighted = highlightedOutcomeId === outcome.id;
            const pts = buildPoints(series, w, h, PAD_L, PAD_R, PAD_T, PAD_B);
            const opacity = isHidden
              ? 0
              : isHighlighted || !highlightedOutcomeId
                ? 1
                : 0.2;
            const strokeW = isHighlighted ? 2 : 1.2;
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
              strokeWidth="0.6"
              strokeDasharray="2 3"
            />
          )}
        </svg>

        {/* Hover tooltip */}
        {tooltip && (
          <div
            className="pointer-events-none absolute top-2 z-10 w-44 rounded-lg border border-slate-700 bg-slate-900/95 p-2 text-xs shadow-lg backdrop-blur-sm"
            style={{
              left:
                tooltip.clientX > dims.w * 0.6
                  ? tooltip.clientX - 188
                  : tooltip.clientX + 12,
            }}
          >
            {tooltip.items.slice(0, 6).map((item) => (
              <div
                key={item.name}
                className="flex items-center justify-between gap-2 py-0.5"
              >
                <span className="flex min-w-0 items-center gap-1.5">
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
            {tooltip.items.length > 6 && (
              <p className="mt-1 text-slate-600">
                +{tooltip.items.length - 6} more
              </p>
            )}
          </div>
        )}
      </div>

      {/* Controls row: timescale buttons */}
      <div className="flex items-center justify-end">
        <div className="inline-flex items-center gap-1 rounded-lg border border-slate-800 bg-slate-950/65 p-1 text-[12px]">
          {TIMESCALES.map((key) => (
            <button
              key={key}
              type="button"
              onClick={() => setTimescale(key)}
              className={`rounded px-2 py-0.5 transition ${
                timescale === key
                  ? "bg-slate-700 text-slate-100"
                  : "text-slate-500 hover:bg-slate-800/70 hover:text-slate-300"
              }`}
            >
              {key}
            </button>
          ))}
        </div>
      </div>

      {/* Legend */}
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
