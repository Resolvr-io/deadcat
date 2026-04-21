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
import { fullContractSats } from "../../utils-react/market";

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

const MIN_SERIES_SEPARATION = 6.2;

// ── Helper functions ────────────────────────────────────────────────

function markerSvg(
  x: number,
  y: number,
  fill: string,
  scale = 1,
): React.JSX.Element {
  const width = MARKER_WIDTH * scale;
  const height = MARKER_HEIGHT * scale;
  return (
    <g
      transform={`translate(${x - width / 2} ${y - height / 2}) scale(${width / MARKER_VIEWBOX_WIDTH} ${height / MARKER_VIEWBOX_HEIGHT})`}
    >
      <path d={CHART_LOGO_PATH} fill={fill} />
    </g>
  );
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

function separateSeriesY(
  yesYRaw: number,
  noYRaw: number,
  plotTop: number,
  plotBottom: number,
): { yesY: number; noY: number } {
  let yesY = yesYRaw;
  let noY = noYRaw;
  const gap = Math.abs(noY - yesY);
  if (gap < MIN_SERIES_SEPARATION) {
    const mid = (yesY + noY) / 2;
    yesY = mid - MIN_SERIES_SEPARATION / 2;
    noY = mid + MIN_SERIES_SEPARATION / 2;
  }
  const minY = plotTop + 0.9;
  const maxY = plotBottom - 0.9;
  if (yesY < minY) {
    const shift = minY - yesY;
    yesY += shift;
    noY += shift;
  }
  if (noY > maxY) {
    const shift = noY - maxY;
    yesY -= shift;
    noY -= shift;
  }
  return { yesY, noY };
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

function LegendIcon({ fill }: { fill: string }) {
  return (
    <svg
      viewBox="0 0 260 267"
      className="h-[11px] w-[11px] shrink-0"
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
  const readoutRailWidth = isHomeChart ? 18 : 22;
  const plotRight = chartWidth - axisTickGutter - readoutRailWidth;
  const plotTop = 2.5;
  const plotBottom = chartHeight - 2.5;
  const plotXSpan = plotRight - plotLeft;
  const plotYSpan = plotBottom - plotTop;

  const yFromProbability = useCallback(
    (price: number): number => plotBottom - price * plotYSpan,
    [plotBottom, plotYSpan],
  );

  // ── Build series points ───────────────────────────────────────────
  const { yesPoints, noPoints } = useMemo(() => {
    const separatedPoints = yesSeries.map((price, idx) => {
      if (price === null) return null;
      const t = pointCount === 1 ? 1 : idx / (pointCount - 1);
      const x = plotLeft + t * plotXSpan;
      const separated = separateSeriesY(
        yFromProbability(price),
        yFromProbability(1 - price),
        plotTop,
        plotBottom,
      );
      return { x, yesY: separated.yesY, noY: separated.noY };
    });

    const visible = separatedPoints.filter(
      (p): p is { x: number; yesY: number; noY: number } => p !== null,
    );

    return {
      yesPoints: visible.map((p) => ({ x: p.x, y: p.yesY })),
      noPoints: visible.map((p) => ({ x: p.x, y: p.noY })),
    };
  }, [yesSeries, pointCount, plotXSpan, plotBottom, yFromProbability]);

  const yesLinePoints = yesPoints
    .map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`)
    .join(" ");
  const noLinePoints = noPoints
    .map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`)
    .join(" ");

  // Guide lines
  const guideLineYs = [0, 25, 50, 75, 100].map((level) =>
    yFromProbability(level / 100),
  );

  // Endpoints
  const defaultPoint = { x: plotRight, y: plotTop + plotYSpan / 2 };
  const yesEnd = yesPoints[yesPoints.length - 1] ?? defaultPoint;
  const noEnd = noPoints[noPoints.length - 1] ?? defaultPoint;
  const yesPct = Math.round(displayedYes * 100);
  const noPct = 100 - yesPct;

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

  const separatedHover =
    hoverActive && hoverYesValue !== null
      ? separateSeriesY(
          yFromProbability(hoverYesValue),
          yFromProbability(1 - hoverYesValue),
          plotTop,
          plotBottom,
        )
      : null;
  const yesHover =
    hoverActive && separatedHover
      ? { x: hoverX, y: separatedHover.yesY }
      : null;
  const noHover =
    hoverActive && separatedHover ? { x: hoverX, y: separatedHover.noY } : null;

  const hoverBlockHeight = Math.round(startBlockHeight + scaleBlocks * hoverT);
  const hoverYesPct =
    hoverYesValue === null ? 0 : Math.round(hoverYesValue * 100);
  const hoverNoPct = 100 - hoverYesPct;
  const endpointOpacity = hoverActive ? "0.4" : "1";
  const showCurrentPulse = !hoverActive || hoverT > 0.985;
  const fadeX = hoverX;
  const fadeW = Math.max(0, plotRight - fadeX);
  const hoverAvailable = hoverActive && yesHover !== null && noHover !== null;

  // ── Paw trails skip zones ─────────────────────────────────────────
  const pawSkipZones = useMemo(
    () => [
      { x: yesEnd.x, y: yesEnd.y, r: 3.5 },
      { x: noEnd.x, y: noEnd.y, r: 3.5 },
      ...(hoverAvailable
        ? [
            { x: yesHover?.x, y: yesHover?.y, r: 3.3 },
            { x: noHover?.x, y: noHover?.y, r: 3.3 },
          ]
        : []),
    ],
    [yesEnd, noEnd, hoverAvailable, yesHover, noHover],
  );

  // ── Readout positioning ───────────────────────────────────────────
  const readoutHoverOffset = isHomeChart ? 8 : 9;
  const readoutRestOffset = isHomeChart ? 6.2 : 6.8;
  const readoutMinX = plotLeft + 4;
  const readoutMaxX = chartWidth - axisTickGutter - 2.2;
  const readoutAnchorX = hoverActive
    ? hoverX + readoutHoverOffset
    : yesEnd.x + readoutRestOffset;
  const readoutX = Math.max(readoutMinX, Math.min(readoutMaxX, readoutAnchorX));
  const readoutLabelFont = isHomeChart ? 4.8 : 5.2;
  const readoutPctFont = isHomeChart ? 9.6 : 10.4;
  const readoutLineGap = isHomeChart ? 0.86 : 0.95;
  const readoutBlockHeight = readoutLabelFont + readoutLineGap + readoutPctFont;
  const readoutStrokeWidth = isHomeChart ? 0.24 : 0.28;
  const readoutTokenOffsetY = readoutLabelFont + 0.96;

  const clampReadoutTop = useCallback(
    (y: number): number =>
      Math.max(
        plotTop + 0.6,
        Math.min(plotBottom - readoutBlockHeight - 0.6, y),
      ),
    [plotBottom, readoutBlockHeight],
  );

  const noAnchorY = hoverAvailable ? noHover?.y : noEnd.y;
  const yesAnchorY = hoverAvailable ? yesHover?.y : yesEnd.y;
  let readoutNoTop = clampReadoutTop(noAnchorY - (readoutLabelFont + 0.8));
  let readoutYesTop = clampReadoutTop(yesAnchorY - (readoutLabelFont + 0.8));
  const minReadoutGap = readoutBlockHeight + 1.4;
  if (readoutNoTop - readoutYesTop < minReadoutGap) {
    const mid = (readoutNoTop + readoutYesTop) / 2;
    readoutNoTop = mid + minReadoutGap / 2;
    readoutYesTop = mid - minReadoutGap / 2;
  }
  readoutNoTop = clampReadoutTop(readoutNoTop);
  readoutYesTop = clampReadoutTop(readoutYesTop);
  if (readoutNoTop - readoutYesTop < minReadoutGap) {
    readoutNoTop = clampReadoutTop(readoutYesTop + minReadoutGap);
  }
  const readoutNoLabelY = readoutNoTop + readoutTokenOffsetY;
  const readoutYesLabelY = readoutYesTop + readoutTokenOffsetY;
  const readoutNoPctY = readoutNoLabelY + readoutLineGap + readoutPctFont;
  const readoutYesPctY = readoutYesLabelY + readoutLineGap + readoutPctFont;
  const readoutNoPct = hasPrice ? (hoverActive ? hoverNoPct : noPct) : null;
  const readoutYesPct = hasPrice ? (hoverActive ? hoverYesPct : yesPct) : null;
  const legendNoPct = hasPrice ? (hoverActive ? hoverNoPct : noPct) : null;
  const legendYesPct = hasPrice ? (hoverActive ? hoverYesPct : yesPct) : null;

  // ── Hover time box ────────────────────────────────────────────────
  const hoverTimeX = Math.max(plotLeft + 18, Math.min(plotRight - 18, hoverX));
  const hoverTimeText = blockHeightToHoverLabel(
    hoverBlockHeight,
    market.currentHeight || hoverBlockHeight,
  );
  const hoverTimeFontSize = isHomeChart ? 7.8 : 8.4;
  const hoverTimeStrokeWidth = isHomeChart ? 0.16 : 0.2;
  const hoverTimeBoxHeight = 15.8;
  const hoverTimeBoxWidth = Math.max(
    70,
    Math.min(178, hoverTimeText.length * 3.4 + 18),
  );
  const hoverTimeBoxX = Math.max(
    plotLeft + 1.2,
    Math.min(
      plotRight - hoverTimeBoxWidth - 1.2,
      hoverTimeX - hoverTimeBoxWidth / 2,
    ),
  );
  const hoverTimeTextX = hoverTimeBoxX + hoverTimeBoxWidth / 2;
  const hoverTimeBoxY = plotTop + 0.25;
  const hoverTimeTextY = hoverTimeBoxY + hoverTimeBoxHeight / 2 + 2.8;

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
    () => buildPawTrail(yesPoints, "#3fbcae", market.isLive, pawSkipZones),
    [yesPoints, market.isLive, pawSkipZones],
  );
  const noPawTrail = useMemo(
    () => buildPawTrail(noPoints, "#e06b7f", market.isLive, pawSkipZones),
    [noPoints, market.isLive, pawSkipZones],
  );

  const fc = fullContractSats(market);

  return (
    <div style={{ fontVariantNumeric: "tabular-nums" }}>
      <div
        className={`relative ${isHomeChart ? "h-[17.5rem]" : "h-[19.5rem]"} rounded-xl border border-slate-800 bg-slate-950/60 p-3`}
      >
        {/* Legend bar */}
        <div className="mb-2 flex items-center gap-4 text-[14px] font-medium text-slate-300">
          <span className="inline-flex items-center gap-1 text-slate-200">
            <LegendIcon fill="#5eead4" />
            Yes {legendYesPct != null ? `${legendYesPct}%` : "\u2014"}
          </span>
          <span className="inline-flex items-center gap-1 text-slate-200">
            <LegendIcon fill="#fb7185" />
            No {legendNoPct != null ? `${legendNoPct}%` : "\u2014"}
          </span>
          <span className="text-slate-500">Yes + No = {fc} sats</span>
          {market.isLive && (
            <span className="inline-flex items-center gap-1 text-xs font-semibold text-rose-400">
              <span className="liveIndicatorDot" />
              Live · Round 1
            </span>
          )}
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
            {/* Guide lines */}
            {guideLineYs.map((y) => (
              <line
                key={y}
                x1="0"
                y1={y}
                x2={chartWidth}
                y2={y}
                stroke="#64748b"
                strokeOpacity="0.24"
                strokeWidth="0.28"
                strokeDasharray="0.45 2.15"
              />
            ))}

            {/* Series lines */}
            {hasPrice && (
              <>
                <polyline
                  fill="none"
                  stroke="#5eead4"
                  strokeOpacity="0.64"
                  strokeWidth="1.08"
                  points={yesLinePoints}
                />
                <polyline
                  fill="none"
                  stroke="#fb7185"
                  strokeOpacity="0.6"
                  strokeWidth="1.08"
                  points={noLinePoints}
                />
                {yesPawTrail}
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
            {hasPrice && showCurrentPulse && (
              <>
                {pulseSvg(yesEnd.x, yesEnd.y, "chartLivePulseYes")}
                {pulseSvg(noEnd.x, noEnd.y, "chartLivePulseNo")}
              </>
            )}

            {/* Endpoint markers */}
            {hasPrice && (
              <g opacity={endpointOpacity}>
                {markerSvg(yesEnd.x, yesEnd.y, "#5eead4")}
                {markerSvg(noEnd.x, noEnd.y, "#fb7185")}
              </g>
            )}

            {/* Hover markers */}
            {hoverAvailable && (
              <>
                {markerSvg(yesHover?.x, yesHover?.y, "#5eead4", 1.16)}
                {markerSvg(noHover?.x, noHover?.y, "#fb7185", 1.16)}
              </>
            )}

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

            {/* Readout labels */}
            {readoutNoPct != null && (
              <>
                <text
                  x={readoutX}
                  y={readoutNoLabelY}
                  fill="#fda4af"
                  fontSize={readoutLabelFont}
                  fontWeight="520"
                  style={{
                    paintOrder: "stroke",
                    stroke: "#020617",
                    strokeWidth: readoutStrokeWidth,
                    strokeOpacity: 0.82,
                  }}
                >
                  NO
                </text>
                <text
                  x={readoutX}
                  y={readoutNoPctY}
                  fill="#f98fa2"
                  fontSize={readoutPctFont}
                  fontWeight="560"
                  style={{
                    paintOrder: "stroke",
                    stroke: "#020617",
                    strokeWidth: readoutStrokeWidth,
                    strokeOpacity: 0.82,
                  }}
                >
                  {readoutNoPct}%
                </text>
                <text
                  x={readoutX}
                  y={readoutYesLabelY}
                  fill="#99f6e4"
                  fontSize={readoutLabelFont}
                  fontWeight="520"
                  style={{
                    paintOrder: "stroke",
                    stroke: "#020617",
                    strokeWidth: readoutStrokeWidth,
                    strokeOpacity: 0.82,
                  }}
                >
                  YES
                </text>
                <text
                  x={readoutX}
                  y={readoutYesPctY}
                  fill="#84f4cb"
                  fontSize={readoutPctFont}
                  fontWeight="560"
                  style={{
                    paintOrder: "stroke",
                    stroke: "#020617",
                    strokeWidth: readoutStrokeWidth,
                    strokeOpacity: 0.82,
                  }}
                >
                  {readoutYesPct}%
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

        {/* Y-axis labels */}
        <div
          className="pointer-events-none absolute right-1 top-10 bottom-8 flex flex-col justify-between text-[12px] font-normal text-slate-500"
          style={{ textShadow: "0 1px 1px rgba(2, 6, 23, 0.35)" }}
        >
          <span>100%</span>
          <span>75%</span>
          <span>50%</span>
          <span>25%</span>
          <span>0%</span>
        </div>

        {/* X-axis labels */}
        <div
          className="pointer-events-none absolute inset-x-3 bottom-1 flex items-center justify-between text-[12px] font-normal text-slate-500"
          style={{ textShadow: "0 1px 1px rgba(2, 6, 23, 0.35)" }}
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
          {(["1h", "4h", "1d", "3d", "7d"] as const).map((key) => (
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
              {key}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
