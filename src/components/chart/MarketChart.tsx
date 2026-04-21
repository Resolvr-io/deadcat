import type React from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "../../store";
import type { Market, PriceHistoryEntry } from "../../types";
import {
  blockHeightToHoverLabel,
  buildChartFromHistory,
  buildChartSeriesData,
  lastDefinedChartProbability,
  sampleChartProbabilityAtFraction,
} from "../../utils-react/chart-series";

// ── Constants ───────────────────────────────────────────────────────

const CHART_LOGO_PATH =
  "M0.146484 9.04605C0.146484 1.23441 10.9146 -3.16002 16.7881 2.6984L86.5566 71.7336C100.142 68.0294 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0306 173.453 71.7365L243.212 2.71207C249.085 -3.14676 259.854 1.24698 259.854 9.05875V161.26C259.949 162.835 260 164.42 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C1.54644e-06 164.42 0.0506677 162.835 0.146484 161.26V9.04605Z";

const MARKER_VIEWBOX_WIDTH = 260;
const MARKER_VIEWBOX_HEIGHT = 267;
const MARKER_WIDTH = 6.1;
const MARKER_HEIGHT = (MARKER_WIDTH * 267) / 260;

const PAW_VIEWBOX = { w: 90, h: 78.98 };
const PAW_PATHS = [
  "M26.62,28.27c4.09,2.84,9.4,2.58,12.27-.69,2.3-2.63,3.06-5.82,3.08-10-.35-5.03-1.89-10.34-6.28-14.44C29.51-2.63,21.1-.1,19.06,8.08c-1.74,6.91,1.71,16.11,7.56,20.18h0Z",
  "M22.98,41.99c.21-1.73.04-3.62-.43-5.3-1.46-5.21-4-9.77-9.08-12.33C7.34,21.27-.31,24.39,0,32.36c-.03,7.11,5.17,14.41,11.8,16.58,5.57,1.82,10.49-1.16,11.17-6.95h0Z",
  "M63.4,28.27c5.85-4.06,9.3-13.26,7.57-20.19C68.92-.12,60.51-2.64,54.33,3.13c-4.4,4.1-5.93,9.41-6.28,14.44.02,4.18.78,7.37,3.08,10,2.87,3.28,8.17,3.54,12.27.7h0Z",
  "M76.54,24.36c-5.08,2.56-7.62,7.12-9.08,12.33-.47,1.68-.63,3.57-.43,5.3.69,5.79,5.61,8.77,11.16,6.96,6.63-2.17,11.83-9.47,11.8-16.58.32-7.99-7.32-11.1-13.45-8.01h0Z",
  "M65.95,49.84c-2.36-2.86-4.3-6.01-6.45-9.02-.89-1.24-1.8-2.47-2.78-3.65-2.76-3.35-7.24-5.02-11.72-5.02s-8.96,1.68-11.72,5.02c-.98,1.19-1.89,2.41-2.78,3.65-2.15,3.01-4.08,6.15-6.45,9.02-1.77,2.15-4.25,3.82-6.11,5.92-4.14,4.69-4.72,9.96-1.94,15.3,2.79,5.37,8.01,7.6,14.41,7.9,4.82.23,9.23-1.95,13.98-2.16.22-.01.42-.01.62-.01s.4,0,.61.01c4.75.21,9.16,2.38,13.98,2.16,6.39-.3,11.62-2.53,14.41-7.9,2.77-5.34,2.2-10.61-1.94-15.3-1.87-2.1-4.35-3.77-6.12-5.92h0Z",
];

// ── Helper functions ────────────────────────────────────────────────

function markerSvg(
  x: number,
  y: number,
  fill: string,
  scale = 1,
  flip = false,
): React.JSX.Element {
  const width = MARKER_WIDTH * scale;
  const height = MARKER_HEIGHT * scale;
  const inner = (
    <g
      transform={`translate(${x - width / 2} ${y - height / 2}) scale(${width / MARKER_VIEWBOX_WIDTH} ${height / MARKER_VIEWBOX_HEIGHT})`}
    >
      <path d={CHART_LOGO_PATH} fill={fill} />
    </g>
  );
  if (!flip) return inner;
  return <g transform={`rotate(180 ${x} ${y})`}>{inner}</g>;
}

function pulseSvg(x: number, y: number, toneClass: string): React.JSX.Element {
  const pulseBaseScale = (MARKER_WIDTH * 0.82) / MARKER_VIEWBOX_WIDTH;
  const markerCenterX = MARKER_VIEWBOX_WIDTH / 2;
  const markerCenterY = MARKER_VIEWBOX_HEIGHT / 2;
  return (
    <g className={toneClass} transform={`translate(${x} ${y})`}>
      <g transform={`scale(${pulseBaseScale})`}>
        <g className="chartLivePulseScale">
          <path
            className="chartLivePulsePath"
            d={CHART_LOGO_PATH}
            transform={`translate(${-markerCenterX} ${-markerCenterY})`}
          />
        </g>
      </g>
    </g>
  );
}

type PointXY = { x: number; y: number };

function buildPawTrail(
  points: PointXY[],
  fill: string,
  isLive: boolean,
  skipZones: Array<{ x: number; y: number; r: number }>,
): React.JSX.Element[] {
  const step = 28;
  const startInset = 0;
  const endInset = 14;
  const pawScale = 0.94;
  const pawOpacity = isLive ? 0.68 : 0.54;

  type Segment = {
    from: PointXY;
    dx: number;
    dy: number;
    len: number;
    cumulativeStart: number;
  };
  const segments: Segment[] = [];
  let cumulative = 0;
  for (let i = 0; i < points.length - 1; i += 1) {
    const from = points[i];
    const to = points[i + 1];
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const len = Math.hypot(dx, dy);
    if (len < 0.001) continue;
    segments.push({ from, dx, dy, len, cumulativeStart: cumulative });
    cumulative += len;
  }
  if (segments.length === 0) return [];

  const totalLen = cumulative;
  const distStart = Math.min(startInset, totalLen);
  const distEnd = Math.max(distStart, totalLen - endInset);

  const pawDistances: number[] = [];
  for (let dist = distStart; dist <= distEnd; dist += step) {
    pawDistances.push(dist);
  }
  if (
    pawDistances.length === 0 ||
    distEnd - pawDistances[pawDistances.length - 1] > step * 0.35
  ) {
    pawDistances.push(distEnd);
  }

  const result: React.JSX.Element[] = [];
  let markIndex = 0;

  for (const dist of pawDistances) {
    const segment =
      segments.find(
        (seg) =>
          dist >= seg.cumulativeStart && dist <= seg.cumulativeStart + seg.len,
      ) ?? segments[segments.length - 1];
    const within = dist - segment.cumulativeStart;
    const t = Math.max(0, Math.min(1, within / segment.len));
    const baseX = segment.from.x + segment.dx * t;
    const baseY = segment.from.y + segment.dy * t;
    const uy = segment.dy / segment.len;
    const nx = -uy;
    const ny = segment.dx / segment.len;
    const lateralOffset = markIndex % 2 === 0 ? 1.05 : -1.05;
    const x = baseX + nx * lateralOffset;
    const y = baseY + ny * lateralOffset;
    const heading = (Math.atan2(segment.dy, segment.dx) * 180) / Math.PI;
    const angle = heading + 90 + (markIndex % 2 === 0 ? 9 : -9);

    const shouldSkip = skipZones.some((zone) => {
      const ddx = x - zone.x;
      const ddy = y - zone.y;
      return ddx * ddx + ddy * ddy <= zone.r * zone.r;
    });

    if (!shouldSkip) {
      const s = pawScale * (5.2 / PAW_VIEWBOX.w);
      result.push(
        <g
          key={`paw-${fill}-${markIndex}`}
          transform={`translate(${x} ${y}) rotate(${angle}) scale(${s})`}
          opacity={pawOpacity}
        >
          {PAW_PATHS.map((d) => (
            <path
              key={d}
              d={d}
              transform={`translate(${-PAW_VIEWBOX.w / 2} ${-PAW_VIEWBOX.h / 2})`}
              fill={fill}
            />
          ))}
        </g>,
      );
    }
    markIndex += 1;
  }

  return result;
}

// ── Legend icon ──────────────────────────────────────────────────────

function LegendIcon({ fill, flip = false }: { fill: string; flip?: boolean }) {
  return (
    <svg
      viewBox="0 0 260 267"
      className={`h-[11px] w-[11px] shrink-0${flip ? " rotate-180" : ""}`}
      aria-hidden="true"
    >
      <path d={CHART_LOGO_PATH} fill={fill} />
    </svg>
  );
}

// ── Main chart component ────────────────────────────────────────────

export default function MarketChart({
  market,
  priceHistory,
  mode = "detail",
}: {
  market: Market;
  priceHistory: PriceHistoryEntry[];
  mode?: "home" | "detail";
}) {
  const chartTimescale = useStore((s) => s.chartTimescale);
  const chartHoverMarketId = useStore((s) => s.chartHoverMarketId);
  const chartHoverX = useStore((s) => s.chartHoverX);
  const fallbackAspect = useStore((s) =>
    mode === "home" ? s.chartAspectHome : s.chartAspectDetail,
  );

  const hoverRef = useRef<HTMLDivElement>(null);
  const svgContainerRef = useRef<HTMLDivElement>(null);
  const isHomeChart = mode === "home";
  const [showYesLine, setShowYesLine] = useState(true);
  const [showNoLine, setShowNoLine] = useState(false);
  const toggleYes = () => {
    if (showYesLine && !showNoLine) return;
    setShowYesLine((v) => !v);
  };
  const toggleNo = () => {
    if (showNoLine && !showYesLine) return;
    setShowNoLine((v) => !v);
  };

  // Measure the actual SVG container so the viewBox always matches the pixel
  // ratio exactly — prevents non-uniform stretching with preserveAspectRatio="none".
  const [measuredAspect, setMeasuredAspect] = useState<number | null>(null);
  useEffect(() => {
    const el = svgContainerRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      const { width, height } = entry.contentRect;
      if (height > 0) setMeasuredAspect(width / height);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const chartAspect = measuredAspect ?? fallbackAspect;

  const hasHistory = priceHistory.length > 0;
  const hasPrice = market.yesPrice != null || hasHistory;
  const fallbackYes = market.yesPrice ?? 0.5;

  const seriesData = useMemo(
    () =>
      hasHistory
        ? buildChartFromHistory(market, priceHistory, chartTimescale)
        : buildChartSeriesData(market, chartTimescale),
    [market, priceHistory, hasHistory, chartTimescale],
  );

  const displayedYes = lastDefinedChartProbability(seriesData, fallbackYes);
  const { pointCount, scaleBlocks, startBlockHeight, xLabels, yesSeries } =
    seriesData;

  // ── Layout constants ──────────────────────────────────────────────
  const chartHeight = 100;
  const clampedAspect = Math.max(1.2, Math.min(8, chartAspect));
  const chartWidth = Math.round(chartHeight * clampedAspect);
  const plotLeft = 2;
  const axisTickGutter = isHomeChart ? 22 : 24;
  const plotRight = chartWidth - axisTickGutter;
  // Reserve top strip for the hover timestamp box (height 15.8 + 2 gap)
  const hoverTimeBoxHeight = 10.5;
  const plotTop = hoverTimeBoxHeight + 2;
  const plotBottom = chartHeight - 2.5;
  const plotXSpan = plotRight - plotLeft;
  const plotYSpan = plotBottom - plotTop;

  const yFromProbability = useCallback(
    (price: number): number => plotBottom - price * plotYSpan,
    [plotBottom, plotYSpan],
  );

  // ── Build series points ───────────────────────────────────────────
  const yesPoints = useMemo(() => {
    return yesSeries
      .map((price, idx) => {
        if (price === null) return null;
        const t = pointCount === 1 ? 1 : idx / (pointCount - 1);
        const x = plotLeft + t * plotXSpan;
        return { x, y: yFromProbability(price) };
      })
      .filter((p): p is { x: number; y: number } => p !== null);
  }, [yesSeries, pointCount, plotXSpan, yFromProbability]);

  const yesLinePoints = yesPoints
    .map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`)
    .join(" ");

  const noPoints = useMemo(() => {
    return yesSeries
      .map((price, idx) => {
        if (price === null) return null;
        const t = pointCount === 1 ? 1 : idx / (pointCount - 1);
        const x = plotLeft + t * plotXSpan;
        return { x, y: yFromProbability(1 - price) };
      })
      .filter((p): p is { x: number; y: number } => p !== null);
  }, [yesSeries, pointCount, plotXSpan, yFromProbability]);

  const noLinePoints = noPoints
    .map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`)
    .join(" ");

  // Endpoints
  const defaultPoint = { x: plotRight, y: plotTop + plotYSpan / 2 };
  const yesEnd = yesPoints[yesPoints.length - 1] ?? defaultPoint;
  const noEnd = noPoints[noPoints.length - 1] ?? defaultPoint;
  const yesPct = Math.round(displayedYes * 100);
  const noPct = Math.round((1 - displayedYes) * 100);

  // ── Hover state ───────────────────────────────────────────────────
  const hoverRequested =
    chartHoverMarketId === market.id && chartHoverX !== null;
  const hoverX = hoverRequested
    ? Math.max(plotLeft, Math.min(plotRight, chartHoverX as number))
    : yesEnd.x;
  const hoverT =
    plotXSpan <= 0
      ? 1
      : Math.max(0, Math.min(1, (hoverX - plotLeft) / plotXSpan));
  const hoverYesValue = sampleChartProbabilityAtFraction(seriesData, hoverT);
  const hoverActive = hoverRequested && hoverYesValue !== null;

  const yesHover =
    hoverActive && hoverYesValue !== null
      ? { x: hoverX, y: yFromProbability(hoverYesValue) }
      : null;

  const hoverNoValue =
    hoverActive && hoverYesValue !== null ? 1 - hoverYesValue : null;
  const noHoverPoint =
    hoverNoValue !== null
      ? { x: hoverX, y: yFromProbability(hoverNoValue) }
      : null;

  const hoverBlockHeight = Math.round(startBlockHeight + scaleBlocks * hoverT);
  const hoverYesPct =
    hoverYesValue === null ? 0 : Math.round(hoverYesValue * 100);
  const hoverNoPct = hoverNoValue === null ? 0 : Math.round(hoverNoValue * 100);
  const endpointOpacity = hoverActive ? "0.4" : "1";
  const showCurrentPulse = !hoverActive || hoverT > 0.985;
  const fadeX = hoverX;
  const fadeW = Math.max(0, plotRight - fadeX);
  const hoverAvailable = hoverActive && yesHover !== null;

  // ── Paw trails skip zones ─────────────────────────────────────────
  const pawSkipZones = useMemo(
    () => [
      { x: yesEnd.x, y: yesEnd.y, r: 3.5 },
      ...(showNoLine ? [{ x: noEnd.x, y: noEnd.y, r: 3.5 }] : []),
      ...(hoverAvailable ? [{ x: yesHover?.x, y: yesHover?.y, r: 3.3 }] : []),
      ...(hoverAvailable && showNoLine && noHoverPoint
        ? [{ x: noHoverPoint.x, y: noHoverPoint.y, r: 3.3 }]
        : []),
    ],
    [yesEnd, noEnd, showNoLine, hoverAvailable, yesHover, noHoverPoint],
  );

  // ── Readout positioning ───────────────────────────────────────────
  const readoutHoverOffset = isHomeChart ? 8 : 9;
  const readoutRestOffset = isHomeChart ? 6.2 : 6.8;
  const readoutMinX = plotLeft + 4;
  const readoutMaxX = chartWidth - axisTickGutter - 2.2;
  const readoutFont = isHomeChart ? 5.2 : 5.6;
  const readoutPadX = 2.5;
  const readoutPadY = 1.4;
  const readoutBgHeight = readoutFont + readoutPadY * 2;
  // "Yes 100%" is the widest text (8 chars); estimate width from char count
  const readoutEstWidth = Math.round(readoutFont * 0.6 * 8 + readoutPadX * 2);
  const readoutBlockHeight = readoutBgHeight;

  const readoutAnchorPoint = hoverActive ? hoverX : yesEnd.x;
  const readoutOffset = hoverActive ? readoutHoverOffset : readoutRestOffset;
  const readoutRightX = readoutAnchorPoint + readoutOffset;
  // Flip to left side when right-side placement would overflow the axis gutter.
  const readoutFlipLeft = readoutRightX + readoutEstWidth > readoutMaxX;
  const readoutX = Math.max(
    readoutMinX,
    readoutFlipLeft
      ? Math.min(
          readoutMaxX - readoutEstWidth,
          readoutAnchorPoint - readoutOffset - readoutEstWidth,
        )
      : Math.min(readoutMaxX - readoutEstWidth, readoutRightX),
  );

  const clampReadoutTop = useCallback(
    (y: number): number =>
      Math.max(
        plotTop + 0.6,
        Math.min(plotBottom - readoutBlockHeight - 0.6, y),
      ),
    [plotTop, plotBottom, readoutBlockHeight],
  );

  const yesAnchorY = hoverAvailable ? yesHover?.y : yesEnd.y;
  const noAnchorY = noHoverPoint ? noHoverPoint.y : noEnd.y;

  // Compute pill positions and push apart if they'd overlap.
  const pillGap = 1.5;
  const minPillSep = readoutBgHeight + pillGap;
  let yesBgY = clampReadoutTop(yesAnchorY - readoutBgHeight / 2);
  let noBgY = clampReadoutTop(noAnchorY - readoutBgHeight / 2);
  if (showNoLine) {
    const sep = noBgY - yesBgY;
    if (Math.abs(sep) < minPillSep) {
      const mid = (yesBgY + noBgY) / 2;
      yesBgY = clampReadoutTop(mid - minPillSep / 2);
      noBgY = clampReadoutTop(mid + minPillSep / 2);
    }
  }
  const readoutBgY = yesBgY;
  const pillTextOffsetY = readoutBgHeight / 2 + readoutFont * 0.35;
  const readoutTextY = readoutBgY + pillTextOffsetY;
  const noTextY = noBgY + pillTextOffsetY;
  const pillCenterX = readoutX + readoutEstWidth / 2;
  const readoutPct = hasPrice ? (hoverActive ? hoverYesPct : yesPct) : null;
  const legendPct = hasPrice ? (hoverActive ? hoverYesPct : yesPct) : null;

  // ── Hover time box ────────────────────────────────────────────────
  const hoverTimeX = Math.max(plotLeft + 18, Math.min(plotRight - 18, hoverX));
  const hoverTimeText = blockHeightToHoverLabel(
    hoverBlockHeight,
    market.currentHeight || hoverBlockHeight,
  );
  const hoverTimeFontSize = isHomeChart ? 5.2 : 5.6;
  const hoverTimeStrokeWidth = isHomeChart ? 0.14 : 0.16;
  const hoverTimeBoxWidth = Math.max(
    48,
    Math.min(130, hoverTimeText.length * 2.3 + 14),
  );
  const hoverTimeBoxX = Math.max(
    plotLeft + 1.2,
    Math.min(
      plotRight - hoverTimeBoxWidth - 1.2,
      hoverTimeX - hoverTimeBoxWidth / 2,
    ),
  );
  const hoverTimeTextX = hoverTimeBoxX + hoverTimeBoxWidth / 2;
  // Timestamp lives in the reserved top strip, above the plot area.
  const hoverTimeBoxY = 0.5;
  const hoverTimeTextY =
    hoverTimeBoxY + hoverTimeBoxHeight / 2 + hoverTimeFontSize * 0.35;

  const volumeLabel = `${market.volumeBtc.toLocaleString(undefined, {
    minimumFractionDigits: market.volumeBtc < 1 ? 2 : 1,
    maximumFractionDigits: 2,
  })} BTC vol`;

  // ── Mouse hover handler ───────────────────────────────────────────
  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const el = hoverRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const fraction = (e.clientX - rect.left) / rect.width;
      const svgX = plotLeft + fraction * plotXSpan;
      useStore.setState({
        chartHoverMarketId: market.id,
        chartHoverX: svgX,
      });
    },
    [market.id, plotXSpan],
  );

  const handleMouseLeave = useCallback(() => {
    useStore.setState({
      chartHoverMarketId: null,
      chartHoverX: null,
    });
  }, []);

  // ── Paw trails (memoized) ─────────────────────────────────────────
  const yesPawTrail = useMemo(
    () =>
      showYesLine
        ? buildPawTrail(yesPoints, "#3fbcae", market.isLive, pawSkipZones)
        : [],
    [showYesLine, yesPoints, market.isLive, pawSkipZones],
  );

  const noPawTrail = useMemo(
    () =>
      showNoLine
        ? buildPawTrail(noPoints, "#f87171", market.isLive, pawSkipZones)
        : [],
    [showNoLine, noPoints, market.isLive, pawSkipZones],
  );

  const noReadoutPct = hasPrice ? (hoverActive ? hoverNoPct : noPct) : null;

  return (
    <div style={{ fontVariantNumeric: "tabular-nums" }}>
      <div
        className={`relative ${isHomeChart ? "h-[17.5rem]" : "h-[19.5rem]"} rounded-xl border border-slate-800 bg-slate-950/60 p-3`}
      >
        {/* Legend bar */}
        <div className="mb-2 flex items-center gap-3 text-[14px] font-medium">
          {showYesLine && (
            <span className="inline-flex items-center gap-1 text-emerald-400">
              <LegendIcon fill="#34d399" />
              {legendPct != null ? `Yes ${legendPct}%` : "\u2014"}
            </span>
          )}
          {showNoLine && (
            <span className="inline-flex items-center gap-1 text-rose-400">
              <LegendIcon fill="#f87171" flip />
              No {noReadoutPct != null ? `${noReadoutPct}%` : "\u2014"}
            </span>
          )}
          {market.isLive && (
            <span className="inline-flex items-center gap-1 text-xs font-semibold text-rose-400">
              <span className="liveIndicatorDot" />
              Live · Round 1
            </span>
          )}
          <div className="ml-auto inline-flex items-center gap-1">
            <button
              type="button"
              onClick={toggleYes}
              className={`inline-flex items-center gap-1 rounded border px-2 py-0.5 text-xs font-semibold transition ${
                showYesLine
                  ? "border-emerald-800 bg-emerald-950/60 text-emerald-400"
                  : "border-slate-700 text-slate-500 hover:text-slate-300"
              }`}
            >
              <LegendIcon fill={showYesLine ? "#34d399" : "#64748b"} />
              Yes
            </button>
            <button
              type="button"
              onClick={toggleNo}
              className={`inline-flex items-center gap-1 rounded border px-2 py-0.5 text-xs font-semibold transition ${
                showNoLine
                  ? "border-rose-800 bg-rose-950/60 text-rose-400"
                  : "border-slate-700 text-slate-500 hover:text-slate-300"
              }`}
            >
              <LegendIcon
                fill={showNoLine ? "#f87171" : "#64748b"}
                flip={showNoLine}
              />
              No
            </button>
          </div>
        </div>

        {/* SVG chart */}
        <div
          ref={svgContainerRef}
          className="pointer-events-none absolute inset-x-3 top-10 bottom-8"
        >
          <svg
            viewBox={`0 0 ${chartWidth} ${chartHeight}`}
            preserveAspectRatio="none"
            className="h-full w-full"
            aria-hidden="true"
          >
            {/* Guide lines + y-axis labels */}
            {[1, 0.75, 0.5, 0.25, 0].map((level) => {
              const y = yFromProbability(level);
              return (
                <g key={level}>
                  <line
                    x1="0"
                    y1={y}
                    x2={chartWidth}
                    y2={y}
                    stroke="#64748b"
                    strokeOpacity="0.24"
                    strokeWidth="0.28"
                    strokeDasharray="0.45 2.15"
                  />
                  <text
                    x={chartWidth - 1}
                    y={y}
                    fill="#64748b"
                    fontSize={isHomeChart ? 4 : 4.5}
                    fontWeight="400"
                    dominantBaseline="middle"
                    textAnchor="end"
                  >
                    {`${level * 100}%`}
                  </text>
                </g>
              );
            })}

            {/* Series lines */}
            {hasPrice && showYesLine && (
              <>
                <polyline
                  fill="none"
                  stroke="#5eead4"
                  strokeOpacity="0.64"
                  strokeWidth="1.08"
                  points={yesLinePoints}
                />
                {yesPawTrail}
              </>
            )}
            {hasPrice && showNoLine && (
              <>
                <polyline
                  fill="none"
                  stroke="#f87171"
                  strokeOpacity="0.64"
                  strokeWidth="1.08"
                  points={noLinePoints}
                />
                {noPawTrail}
              </>
            )}
            {/* Hover fade + crosshair */}
            {hoverAvailable && (
              <>
                <rect
                  x={fadeX}
                  y={plotTop}
                  width={fadeW}
                  height={plotYSpan}
                  fill="#020617"
                  fillOpacity="0.5"
                />
                <line
                  x1={yesHover?.x}
                  y1={plotTop}
                  x2={yesHover?.x}
                  y2={plotBottom}
                  stroke="#e2e8f0"
                  strokeOpacity="0.6"
                  strokeWidth="0.32"
                />
              </>
            )}

            {/* Pulse animation at current point */}
            {hasPrice &&
              showYesLine &&
              showCurrentPulse &&
              pulseSvg(yesEnd.x, yesEnd.y, "chartLivePulseYes")}

            {/* Endpoint markers */}
            {hasPrice && (
              <g opacity={endpointOpacity}>
                {showYesLine && markerSvg(yesEnd.x, yesEnd.y, "#5eead4")}
                {showNoLine && markerSvg(noEnd.x, noEnd.y, "#f87171", 1, true)}
              </g>
            )}

            {/* Hover markers */}
            {hoverAvailable &&
              showYesLine &&
              markerSvg(yesHover?.x, yesHover?.y, "#5eead4", 1.16)}
            {hoverAvailable &&
              showNoLine &&
              noHoverPoint &&
              markerSvg(noHoverPoint.x, noHoverPoint.y, "#f87171", 1.16, true)}

            {/* Hover time box */}
            {hoverAvailable && (
              <>
                <rect
                  x={hoverTimeBoxX}
                  y={hoverTimeBoxY}
                  width={hoverTimeBoxWidth}
                  height={hoverTimeBoxHeight}
                  rx="2.45"
                  fill="#020617"
                  fillOpacity="0.8"
                  stroke="#475569"
                  strokeOpacity="0.56"
                  strokeWidth="0.24"
                />
                <text
                  x={hoverTimeTextX}
                  y={hoverTimeTextY}
                  fill="#dbe7f6"
                  fontSize={hoverTimeFontSize}
                  fontWeight="430"
                  textAnchor="middle"
                  style={{
                    paintOrder: "stroke",
                    stroke: "#020617",
                    strokeWidth: hoverTimeStrokeWidth,
                    strokeOpacity: 0.45,
                  }}
                >
                  {hoverTimeText}
                </text>
              </>
            )}

            {/* Readout pills — hover only */}
            {hoverAvailable && showYesLine && readoutPct != null && (
              <>
                <rect
                  x={readoutX}
                  y={readoutBgY}
                  width={readoutEstWidth}
                  height={readoutBgHeight}
                  rx="2.2"
                  fill="#5eead4"
                  fillOpacity="0.92"
                />
                <text
                  x={pillCenterX}
                  y={readoutTextY}
                  fill="#020617"
                  fontSize={readoutFont}
                  fontWeight="600"
                  textAnchor="middle"
                >
                  Yes {readoutPct}%
                </text>
              </>
            )}
            {hoverAvailable && showNoLine && noReadoutPct != null && (
              <>
                <rect
                  x={readoutX}
                  y={noBgY}
                  width={readoutEstWidth}
                  height={readoutBgHeight}
                  rx="2.2"
                  fill="#f87171"
                  fillOpacity="0.92"
                />
                <text
                  x={pillCenterX}
                  y={noTextY}
                  fill="#020617"
                  fontSize={readoutFont}
                  fontWeight="600"
                  textAnchor="middle"
                >
                  No {noReadoutPct}%
                </text>
              </>
            )}
          </svg>
        </div>

        {/* No price data overlay */}
        {!hasPrice && (
          <div className="pointer-events-none absolute inset-x-3 top-10 bottom-8 flex flex-col items-center justify-center gap-2">
            <svg
              aria-hidden="true"
              viewBox="0 0 260 267"
              className="h-12 w-12 opacity-30"
            >
              <path d={CHART_LOGO_PATH} fill="#64748b" />
            </svg>
            <span className="text-sm font-medium text-slate-600">
              No price data
            </span>
          </div>
        )}

        {/* Hover interaction overlay */}
        <div
          ref={hoverRef}
          role="presentation"
          className="absolute inset-x-3 top-10 bottom-8 z-10"
          onMouseMove={handleMouseMove}
          onMouseLeave={handleMouseLeave}
        />

        {/* X-axis labels */}
        <div
          className="pointer-events-none absolute inset-x-3 bottom-1 flex items-center justify-between text-[12px] font-normal text-slate-500"
          style={{
            textShadow: "0 1px 1px rgba(2, 6, 23, 0.35)",
            // Align last label with plotRight by reserving the same proportion
            // as axisTickGutter occupies in the SVG coordinate space.
            paddingRight: `${(axisTickGutter / chartWidth) * 100}%`,
          }}
        >
          {xLabels.map((label, i) => (
            <span key={i}>{label}</span>
          ))}
        </div>
      </div>

      {/* Volume + timescale controls */}
      <div className="mt-2 flex items-center justify-between">
        <span className="pl-0.5 text-[13px] font-medium text-slate-300">
          {volumeLabel}
        </span>
        <div className="inline-flex items-center gap-1 rounded-lg border border-slate-800 bg-slate-950/65 p-1 text-[12px]">
          {(["1h", "4h", "1d", "3d", "7d", "1M", "all"] as const).map((key) => (
            <button
              type="button"
              key={key}
              onClick={() => useStore.setState({ chartTimescale: key })}
              className={`rounded px-2 py-0.5 transition ${
                chartTimescale === key
                  ? "bg-slate-700 text-slate-100"
                  : "text-slate-500 hover:bg-slate-800/70 hover:text-slate-300"
              }`}
            >
              {key === "all" ? "ALL" : key}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
