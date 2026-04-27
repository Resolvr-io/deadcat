import { useEffect, useRef, useState } from "react";
import { useStore } from "../../store";
import type { Market, MarketGroup } from "../../types";
import {
  formatSettlementDateTime,
  formatTimeRemaining,
} from "../../utils-react/format";
import {
  getEstimatedSettlementDate,
  getPositionContracts,
} from "../../utils-react/market";
import { OUTCOME_COLORS } from "../group/GroupChart";
import { categoryIcon } from "../layout/TopShell";
import { MarketActionsMenu } from "./MarketActionsMenu";

function stateBadge(state: number) {
  const colorMap: Record<number, string> = {
    0: "bg-slate-600/30 text-slate-300",
    1: "bg-sky-500/20 text-sky-300",
    2: "bg-emerald-500/30 text-emerald-200",
    3: "bg-rose-500/30 text-rose-200",
    4: "bg-amber-500/25 text-amber-200",
  };
  const labels: Record<number, string> = {
    0: "Dormant",
    1: "Trading",
    2: "Resolved YES",
    3: "Resolved NO",
    4: "Expired",
  };

  return (
    <span
      className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${colorMap[state] ?? ""}`}
    >
      {labels[state] ?? "Unknown"}
    </span>
  );
}

/** Above the chart: nav, title, stats strip. Matches group market layout. */
export function MarketHeaderTop({ market }: { market: Market }) {
  const blocksLeft = market.expiryHeight - market.currentHeight;
  const closesColor =
    blocksLeft < 2880
      ? "text-rose-400"
      : blocksLeft < 10080
        ? "text-amber-400"
        : "text-slate-200";

  return (
    <>
      {/* Back nav — category + state badge inline, matching group market */}
      <div className="mb-2 flex items-center gap-2">
        <button
          type="button"
          onClick={() => useStore.setState({ view: "home" })}
          className="flex items-center gap-1.5 text-sm text-slate-500 transition hover:text-slate-200"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
            className="h-4 w-4"
          >
            <polyline points="15 18 9 12 15 6" />
          </svg>
          Markets
        </button>
        <div className="flex items-center gap-2">
          <span className="flex items-center gap-1.5 rounded-full bg-slate-800 px-2.5 py-0.5 text-xs text-slate-300">
            {categoryIcon(market.category, "h-3 w-3")}
            {market.category}
          </span>
          {stateBadge(market.state)}
          <MarketActionsMenu market={market} />
        </div>
      </div>

      {/* Title */}
      <h1 className="mb-2 text-2xl font-semibold leading-tight text-slate-100 lg:text-3xl">
        {market.question}
      </h1>

      {/* Stats strip */}
      <div className="mb-8 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-slate-500">
        <span>
          <span className="text-slate-300">
            {market.traderCount > 0 ? market.traderCount.toLocaleString() : "—"}
          </span>{" "}
          traders
        </span>
        <span className="text-slate-700">·</span>
        <span>
          Closes{" "}
          <span className={closesColor}>{formatTimeRemaining(blocksLeft)}</span>
        </span>
      </div>
    </>
  );
}

/** Below the chart: description, resolution metadata, position display. */
export function MarketHeaderBottom({ market }: { market: Market }) {
  const walletData = useStore((s) => s.walletData);

  const estimatedSettlementDate = getEstimatedSettlementDate(market);
  const positions = getPositionContracts(market, walletData);

  return (
    <>
      {/* Description */}
      <p className="mb-4 text-sm text-slate-400">{market.description}</p>

      {/* Resolution metadata — compact single line */}
      <p className="mb-4 text-xs text-slate-500">
        <span className="font-semibold uppercase tracking-wider text-slate-400">
          Resolution
        </span>
        {" · "}Source:{" "}
        <span className="text-slate-300">{market.resolutionSource}</span>
        {" · "}Deadline:{" "}
        <span className="text-slate-300">
          Est. {formatSettlementDateTime(estimatedSettlementDate)}
        </span>
        {" · "}Block:{" "}
        <span className="text-slate-300">
          {market.expiryHeight.toLocaleString()}
        </span>
      </p>

      {/* Position display */}
      {(positions.yes > 0 || positions.no > 0) && (
        <div className="mb-4 flex items-center gap-3 rounded-xl bg-slate-900/40 px-4 py-2 text-sm">
          <span className="text-slate-400">Your position</span>
          {positions.yes > 0 && (
            <span className="rounded bg-emerald-500/20 px-2 py-0.5 font-medium text-emerald-300">
              YES {positions.yes.toLocaleString()}
            </span>
          )}
          {positions.no > 0 && (
            <span className="rounded bg-red-500/20 px-2 py-0.5 font-medium text-red-300">
              NO {positions.no.toLocaleString()}
            </span>
          )}
        </div>
      )}
    </>
  );
}

/** @deprecated Use MarketHeaderTop + MarketHeaderBottom split around the chart. */
export default function MarketHeader({ market }: { market: Market }) {
  return (
    <>
      <MarketHeaderTop market={market} />
      <MarketHeaderBottom market={market} />
    </>
  );
}

/** Hook + ref pair to detect when an element scrolls out of the
 *  viewport. Used by the detail page to know when the full-size
 *  H1 has scrolled past the top so it can render the minimized
 *  sticky bar.
 *
 *  `rootMargin` shifts the intersection root from the actual
 *  viewport. The detail page passes a negative top margin so the
 *  bar trips a little earlier — as soon as the title intrudes
 *  into the bar's reserved area at the top of the viewport,
 *  rather than only after the title is fully scrolled past.
 *
 *  Default `true` so the first-paint state is "title visible,
 *  bar hidden" until the observer fires. */
export function useIsInView<T extends HTMLElement>(
  options: { rootMargin?: string } = {},
): {
  ref: React.RefObject<T | null>;
  inView: boolean;
} {
  const { rootMargin } = options;
  const ref = useRef<T>(null);
  const [inView, setInView] = useState(true);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      ([entry]) => setInView(entry?.isIntersecting ?? false),
      { threshold: 0, rootMargin },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [rootMargin]);
  return { ref, inView };
}

/** Pixels the sticky bar's banner extends past the column on each
 *  side. 16px = half of the 32px (`gap-8`) inter-column gutter, so
 *  the right edge lands at the midpoint of the gap and visually
 *  doesn't crowd the right-column trading panel. The same
 *  symmetric overhang on the left keeps the bar centered around
 *  its content. Compensated with matching extra horizontal
 *  padding so the bar's content (back arrow, title, pills) stays
 *  aligned with the column — only the background / border /
 *  rounded chrome reaches out further. */
const STICKY_BAR_BG_OVERHANG = 16;
/** Built-in horizontal padding the bar's content uses inside the
 *  banner — the original `px-3` Tailwind utility's value. Kept as a
 *  constant so the inline-style padding math stays readable. */
const STICKY_BAR_INNER_PADDING_X = 12;

/** Track an element's bounding-rect left+width so a `position:
 *  fixed` bar can match the host column's horizontal position.
 *  Updates on element resize, page scroll (scrollbar appearance
 *  shifts horizontal layout on some setups), and window resize.
 *  Returns null until the ref attaches, so the consumer can
 *  short-circuit before its first paint and avoid flashing at the
 *  wrong x-coordinate. */
function useElementRect<T extends HTMLElement>(
  ref: React.RefObject<T | null>,
): { left: number; width: number } | null {
  const [rect, setRect] = useState<{ left: number; width: number } | null>(
    null,
  );
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => {
      const r = el.getBoundingClientRect();
      setRect({ left: r.left, width: r.width });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", update);
    };
  }, [ref]);
  return rect;
}

/**
 * Compact title bar that appears at the top of the viewport once
 * the full-size H1 has scrolled past — keeps "what am I commenting
 * on" visible while the user scrolls through the chart, resolution
 * info, and comment list.
 *
 * Uses `position: fixed` (not `sticky`) so it's completely absent
 * from the layout when the title is in view — no leading row of
 * empty space, no flicker when scrolling past. `columnRef` is the
 * left grid column's bounding box so the bar's left+width match
 * that column and don't bleed across the right-column trading
 * panel.
 */
export function StickyMarketHeader({
  market,
  visible,
  columnRef,
}: {
  market: Market;
  visible: boolean;
  columnRef: React.RefObject<HTMLElement | null>;
}) {
  const yesPct =
    market.yesPrice != null ? Math.round(market.yesPrice * 100) : null;
  const noPct = yesPct != null ? 100 - yesPct : null;
  const rect = useElementRect(columnRef);
  if (!visible || rect == null) return null;
  return (
    <div
      // `position: fixed` with computed left/width keeps the bar
      // aligned to the column without depending on layout flow.
      // `.sticky-overlay-safe-top` provides top: 0 + the macOS
      // padding that clears the traffic-light strip while letting
      // the bar's bg fill the strip area.
      // The banner extends `STICKY_BAR_BG_OVERHANG` past the column
      // on each side (background + border + corners only). Inner
      // horizontal padding compensates so the back arrow / title /
      // pills sit at the same x-coords as the column edges — the
      // overhang is purely a chrome flourish.
      style={{
        left: rect.left - STICKY_BAR_BG_OVERHANG,
        width: rect.width + STICKY_BAR_BG_OVERHANG * 2,
        paddingLeft: STICKY_BAR_BG_OVERHANG + STICKY_BAR_INNER_PADDING_X,
        paddingRight: STICKY_BAR_BG_OVERHANG + STICKY_BAR_INNER_PADDING_X,
      }}
      className="sticky-overlay-safe-top fixed z-20 flex items-center gap-3 rounded-lg border border-slate-800/60 bg-slate-950/65 backdrop-blur"
    >
      <button
        type="button"
        onClick={() => useStore.setState({ view: "home" })}
        title="Back to markets"
        className="shrink-0 text-slate-500 transition hover:text-slate-200"
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
          className="h-4 w-4"
        >
          <polyline points="15 18 9 12 15 6" />
        </svg>
      </button>
      <p className="min-w-0 flex-1 truncate text-base font-medium text-slate-100">
        {market.question}
      </p>
      <div className="flex shrink-0 items-center gap-1.5 text-xs">
        {yesPct != null && (
          <span className="rounded-full bg-emerald-500/15 px-2.5 py-1 font-medium text-emerald-300">
            Yes {yesPct}%
          </span>
        )}
        {noPct != null && (
          <span className="rounded-full bg-rose-500/15 px-2.5 py-1 font-medium text-rose-300">
            No {noPct}%
          </span>
        )}
      </div>
    </div>
  );
}

/**
 * Multi-outcome counterpart to `StickyMarketHeader`. Same chrome,
 * same sticky / overlay-safe positioning rules. The right side
 * shows the currently-selected outcome — falling back to the
 * leading outcome (highest yesPrice) when none is explicitly
 * selected — plus a `+N` badge for the rest. Tracking the
 * selection means the bar stays in sync with whichever outcome
 * the user is reading / trading on the right column.
 */
export function StickyGroupHeader({
  group,
  selectedOutcomeId,
  visible,
  columnRef,
}: {
  group: MarketGroup;
  selectedOutcomeId: string | null;
  visible: boolean;
  columnRef: React.RefObject<HTMLElement | null>;
}) {
  // Resolve the featured outcome via index lookup (not just `find`)
  // because the index drives the palette color — the rest of the
  // group UI (chart legend, outcome list, trading panel header)
  // ties an outcome to `OUTCOME_COLORS[index % palette.length]`,
  // and the pill should match so the user reads the bar as the
  // same entity they're trading on.
  const selectedIndex = group.outcomes.findIndex(
    (o) => o.id === selectedOutcomeId,
  );
  const fallbackIndex = group.outcomes.reduce(
    (best, o, i, arr) => (o.yesPrice > arr[best].yesPrice ? i : best),
    0,
  );
  const featuredIndex =
    selectedIndex >= 0
      ? selectedIndex
      : group.outcomes.length > 0
        ? fallbackIndex
        : -1;
  const featured = featuredIndex >= 0 ? group.outcomes[featuredIndex] : null;
  const featuredColor =
    featuredIndex >= 0
      ? OUTCOME_COLORS[featuredIndex % OUTCOME_COLORS.length]
      : null;
  const featuredPct =
    featured != null ? Math.round(featured.yesPrice * 100) : null;
  const otherCount = Math.max(group.outcomes.length - 1, 0);
  const rect = useElementRect(columnRef);
  if (!visible || rect == null) return null;
  return (
    <div
      // Same `position: fixed` + column-rect alignment + banner
      // overhang as `StickyMarketHeader` — see that component for
      // the rationale.
      style={{
        left: rect.left - STICKY_BAR_BG_OVERHANG,
        width: rect.width + STICKY_BAR_BG_OVERHANG * 2,
        paddingLeft: STICKY_BAR_BG_OVERHANG + STICKY_BAR_INNER_PADDING_X,
        paddingRight: STICKY_BAR_BG_OVERHANG + STICKY_BAR_INNER_PADDING_X,
      }}
      className="sticky-overlay-safe-top fixed z-20 flex items-center gap-3 rounded-lg border border-slate-800/60 bg-slate-950/65 backdrop-blur"
    >
      <button
        type="button"
        onClick={() => useStore.setState({ view: "home" })}
        title="Back to markets"
        className="shrink-0 text-slate-500 transition hover:text-slate-200"
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
          className="h-4 w-4"
        >
          <polyline points="15 18 9 12 15 6" />
        </svg>
      </button>
      <p className="min-w-0 flex-1 truncate text-base font-medium text-slate-100">
        {group.title}
      </p>
      {featured != null && featuredPct != null && featuredColor != null && (
        <div className="flex shrink-0 items-center gap-1.5 text-xs">
          {/* Inline style instead of a Tailwind utility because the
              palette is an array of arbitrary hex values, not a
              fixed Tailwind token set. The `1f` suffix on the
              background hex is ~12% alpha — keeps the pill subtle
              while letting the outcome's accent read at a glance. */}
          <span
            className="max-w-[160px] truncate rounded-full px-2.5 py-1 font-medium"
            style={{
              backgroundColor: `${featuredColor}1f`,
              color: featuredColor,
            }}
          >
            {featured.name} {featuredPct}%
          </span>
          {otherCount > 0 && (
            <span className="rounded-full bg-slate-800/60 px-2.5 py-1 font-medium text-slate-400">
              +{otherCount}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
