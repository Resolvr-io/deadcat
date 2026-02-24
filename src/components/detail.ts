import {
  EXECUTION_FEE_RATE,
  SATS_PER_FULL_CONTRACT,
  state,
  WIN_FEE_RATE,
} from "../state.ts";
import type { Market } from "../types.ts";
import { hexToNpub } from "../utils/crypto.ts";
import {
  formatBlockHeight,
  formatEstTime,
  formatProbabilityWithPercent,
  formatSats,
  formatSettlementDateTime,
} from "../utils/format.ts";
import {
  clampContractPriceSats,
  getEstimatedSettlementDate,
  getOrderbookLevels,
  getPathAvailability,
  getPositionContracts,
  getSelectedMarket,
  getTradePreview,
  isExpired,
  stateBadge,
  stateLabel,
} from "../utils/market.ts";

export function chartSkeleton(
  market: Market,
  mode: "home" | "detail" = "detail",
): string {
  const isHomeChart = mode === "home";
  // Outer silhouette only (no face details) for small chart markers
  const chartLogoPath =
    "M0.146484 9.04605C0.146484 1.23441 10.9146 -3.16002 16.7881 2.6984L86.5566 71.7336C100.142 68.0294 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0306 173.453 71.7365L243.212 2.71207C249.085 -3.14676 259.854 1.24698 259.854 9.05875V161.26C259.949 162.835 260 164.42 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C1.54644e-06 164.42 0.0506677 162.835 0.146484 161.26V9.04605Z";
  const markerViewBoxWidth = 260;
  const markerViewBoxHeight = 267;
  const markerCenterX = markerViewBoxWidth / 2;
  const markerCenterY = markerViewBoxHeight / 2;
  const markerWidth = 6.1;
  const markerHeight = (markerWidth * 267) / 260;
  const markerAt = (x: number, y: number, fill: string, scale = 1): string => {
    const width = markerWidth * scale;
    const height = markerHeight * scale;
    return `
    <g transform="translate(${x - width / 2} ${y - height / 2}) scale(${width / markerViewBoxWidth} ${height / markerViewBoxHeight})">
      <path d="${chartLogoPath}" fill="${fill}" />
    </g>
  `;
  };
  const pulseAt = (x: number, y: number, toneClass: string): string => {
    const pulseBaseScale = (markerWidth * 0.82) / markerViewBoxWidth;
    return `
    <g class="${toneClass}" transform="translate(${x} ${y})">
      <g transform="scale(${pulseBaseScale})">
        <g class="chartLivePulseScale">
          <path class="chartLivePulsePath" d="${chartLogoPath}" transform="translate(${-markerCenterX} ${-markerCenterY})" />
        </g>
      </g>
    </g>
  `;
  };
  const legendIcon = (fill: string): string => `
    <svg viewBox="0 0 260 267" class="h-[11px] w-[11px] shrink-0" aria-hidden="true">
      <path d="${chartLogoPath}" fill="${fill}" />
    </svg>
  `;
  // Cat paw SVG paths (viewBox 0 0 90 78.98, centered at 45, 39.49)
  const pawViewBox = { w: 90, h: 78.98 };
  const pawPaths = [
    "M26.62,28.27c4.09,2.84,9.4,2.58,12.27-.69,2.3-2.63,3.06-5.82,3.08-10-.35-5.03-1.89-10.34-6.28-14.44C29.51-2.63,21.1-.1,19.06,8.08c-1.74,6.91,1.71,16.11,7.56,20.18h0Z",
    "M22.98,41.99c.21-1.73.04-3.62-.43-5.3-1.46-5.21-4-9.77-9.08-12.33C7.34,21.27-.31,24.39,0,32.36c-.03,7.11,5.17,14.41,11.8,16.58,5.57,1.82,10.49-1.16,11.17-6.95h0Z",
    "M63.4,28.27c5.85-4.06,9.3-13.26,7.57-20.19C68.92-.12,60.51-2.64,54.33,3.13c-4.4,4.1-5.93,9.41-6.28,14.44.02,4.18.78,7.37,3.08,10,2.87,3.28,8.17,3.54,12.27.7h0Z",
    "M76.54,24.36c-5.08,2.56-7.62,7.12-9.08,12.33-.47,1.68-.63,3.57-.43,5.3.69,5.79,5.61,8.77,11.16,6.96,6.63-2.17,11.83-9.47,11.8-16.58.32-7.99-7.32-11.1-13.45-8.01h0Z",
    "M65.95,49.84c-2.36-2.86-4.3-6.01-6.45-9.02-.89-1.24-1.8-2.47-2.78-3.65-2.76-3.35-7.24-5.02-11.72-5.02s-8.96,1.68-11.72,5.02c-.98,1.19-1.89,2.41-2.78,3.65-2.15,3.01-4.08,6.15-6.45,9.02-1.77,2.15-4.25,3.82-6.11,5.92-4.14,4.69-4.72,9.96-1.94,15.3,2.79,5.37,8.01,7.6,14.41,7.9,4.82.23,9.23-1.95,13.98-2.16.22-.01.42-.01.62-.01s.4,0,.61.01c4.75.21,9.16,2.38,13.98,2.16,6.39-.3,11.62-2.53,14.41-7.9,2.77-5.34,2.2-10.61-1.94-15.3-1.87-2.1-4.35-3.77-6.12-5.92h0Z",
  ];
  const pawTrail = (
    points: Array<{ x: number; y: number }>,
    fill: string,
    skipZones: Array<{ x: number; y: number; r: number }> = [],
  ): string => {
    const step = 28;
    const startInset = 0;
    const endInset = 14;
    const pawScale = 0.94;
    const pawOpacity = market.isLive ? 0.68 : 0.54;
    const segments: Array<{
      from: { x: number; y: number };
      dx: number;
      dy: number;
      len: number;
      cumulativeStart: number;
    }> = [];

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

    if (segments.length === 0) return "";
    const totalLen = cumulative;
    const distStart = Math.min(startInset, totalLen);
    const distEnd = Math.max(distStart, totalLen - endInset);
    let out = "";

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

    let markIndex = 0;
    for (const dist of pawDistances) {
      const segment =
        segments.find(
          (seg) =>
            dist >= seg.cumulativeStart &&
            dist <= seg.cumulativeStart + seg.len,
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
        const dx = x - zone.x;
        const dy = y - zone.y;
        return dx * dx + dy * dy <= zone.r * zone.r;
      });
      if (shouldSkip) {
        markIndex += 1;
        continue;
      }
      const s = pawScale * (5.2 / pawViewBox.w);
      out += `<g transform="translate(${x} ${y}) rotate(${angle}) scale(${s})" opacity="${pawOpacity}">${pawPaths.map((d) => `<path d="${d}" transform="translate(${-pawViewBox.w / 2} ${-pawViewBox.h / 2})" fill="${fill}" />`).join("")}</g>`;
      markIndex += 1;
    }
    return out;
  };

  const yes = market.yesPrice;
  const now = new Date();
  const scaleHoursByKey: Record<"1H" | "3H" | "6H" | "12H" | "1D", number> = {
    "1H": 1,
    "3H": 3,
    "6H": 6,
    "12H": 12,
    "1D": 24,
  };
  const pointCountByKey: Record<"1H" | "3H" | "6H" | "12H" | "1D", number> = {
    "1H": 28,
    "3H": 34,
    "6H": 40,
    "12H": 48,
    "1D": 56,
  };
  const scaleKey = state.chartTimescale;
  const scaleHours = scaleHoursByKey[scaleKey];
  const pointCount = pointCountByKey[scaleKey];
  const totalHours = 24;
  const startTime = new Date(now.getTime() - scaleHours * 60 * 60 * 1000);
  const xLabelFractions =
    scaleHours >= 12 ? [0, 0.25, 0.5, 0.75, 1] : [0, 1 / 3, 2 / 3, 1];
  const xLabels = xLabelFractions.map(
    (fraction) =>
      new Date(
        startTime.getTime() + fraction * (now.getTime() - startTime.getTime()),
      ),
  );
  const xLabelOffsets = xLabelFractions.map(
    (fraction) => scaleHours * (1 - fraction),
  );
  const seed =
    [...market.id].reduce((sum, ch) => sum + ch.charCodeAt(0), 0) % 97;
  const trendSign = seed % 2 === 0 ? 1 : -1;
  const clampProbability = (value: number): number =>
    Math.max(0.02, Math.min(0.98, value));
  let rngState = seed * 1103515245 + 12345;
  const rand = (): number => {
    rngState = (rngState * 1664525 + 1013904223) >>> 0;
    return rngState / 0xffffffff;
  };
  const baseSeriesCount = 24 * 12 + 1;
  const baseSeries: number[] = [];
  const historicalBias = (0.2 + (seed % 5) * 0.02) * trendSign;
  const historicalCenter = clampProbability(yes + historicalBias);
  for (let idx = 0; idx < baseSeriesCount; idx += 1) {
    const t = idx / (baseSeriesCount - 1);
    const transitionStart = 0.88;
    const transitionT =
      t <= transitionStart ? 0 : (t - transitionStart) / (1 - transitionStart);
    const smoothTransition = transitionT * transitionT * (3 - 2 * transitionT);
    const macroAnchor =
      historicalCenter + (yes - historicalCenter) * smoothTransition;
    const microWaveAmp = t > 0.8 ? 0.03 : 0.018;
    const microWave =
      Math.sin((t * 22 + seed * 0.117) * Math.PI * 2) * microWaveAmp;
    const anchor = clampProbability(macroAnchor + microWave);
    if (idx === 0) {
      baseSeries.push(anchor);
      continue;
    }
    const prev = baseSeries[idx - 1];
    const jump = idx % 20 === 0 ? (rand() - 0.5) * 0.18 : 0;
    const driftPull = (anchor - prev) * 0.2;
    const stepNoise =
      idx % 4 === 0 ? (rand() - 0.5) * 0.03 : (rand() - 0.5) * 0.004;
    const next = prev + jump + driftPull + stepNoise;
    if (idx % 3 !== 0) {
      baseSeries.push(clampProbability(prev + (rand() - 0.5) * 0.0028));
      continue;
    }
    baseSeries.push(clampProbability(next));
  }
  baseSeries[baseSeries.length - 1] = yes;
  const windowStartT = Math.max(0, 1 - scaleHours / totalHours);
  const yesSeries = Array.from({ length: pointCount }, (_v, idx) => {
    const localT = pointCount === 1 ? 1 : idx / (pointCount - 1);
    const baseT = windowStartT + localT * (1 - windowStartT);
    const basePosition = baseT * (baseSeriesCount - 1);
    const left = Math.max(
      0,
      Math.min(baseSeriesCount - 1, Math.floor(basePosition)),
    );
    const right = Math.max(
      left,
      Math.min(baseSeriesCount - 1, Math.ceil(basePosition)),
    );
    const mix = Math.max(0, Math.min(1, basePosition - left));
    return baseSeries[left] + (baseSeries[right] - baseSeries[left]) * mix;
  });
  yesSeries[yesSeries.length - 1] = yes;
  const chartAspect = isHomeChart
    ? state.chartAspectHome
    : state.chartAspectDetail;
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
  const yFromProbability = (price: number): number =>
    plotBottom - price * plotYSpan;
  const minSeriesSeparation = 6.2;
  const separateSeriesY = (
    yesYRaw: number,
    noYRaw: number,
  ): { yesY: number; noY: number } => {
    let yesY = yesYRaw;
    let noY = noYRaw;
    const gap = Math.abs(noY - yesY);
    if (gap < minSeriesSeparation) {
      const mid = (yesY + noY) / 2;
      yesY = mid - minSeriesSeparation / 2;
      noY = mid + minSeriesSeparation / 2;
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
  };
  const separatedPoints = yesSeries.map((price, idx) => {
    const t = pointCount === 1 ? 1 : idx / (pointCount - 1);
    const x = plotLeft + t * plotXSpan;
    const separated = separateSeriesY(
      yFromProbability(price),
      yFromProbability(1 - price),
    );
    return {
      x,
      yesY: separated.yesY,
      noY: separated.noY,
    };
  });
  const yesPoints: Array<{ x: number; y: number }> = separatedPoints.map(
    (point) => ({ x: point.x, y: point.yesY }),
  );
  const noPoints: Array<{ x: number; y: number }> = separatedPoints.map(
    (point) => ({ x: point.x, y: point.noY }),
  );
  const yesLinePoints = yesPoints
    .map((point) => `${point.x.toFixed(3)},${point.y.toFixed(3)}`)
    .join(" ");
  const noLinePoints = noPoints
    .map((point) => `${point.x.toFixed(3)},${point.y.toFixed(3)}`)
    .join(" ");
  const guideLineYs = [0, 25, 50, 75, 100].map((level) =>
    yFromProbability(level / 100),
  );
  const guideLines = guideLineYs
    .map(
      (y) =>
        `<line x1="0" y1="${y}" x2="${chartWidth}" y2="${y}" stroke="#64748b" stroke-opacity="0.24" stroke-width="0.28" stroke-dasharray="0.45 2.15" />`,
    )
    .join("");

  const yesEnd = yesPoints[yesPoints.length - 1];
  const noEnd = noPoints[noPoints.length - 1];
  const yesPct = Math.round(yes * 100);
  const noPct = 100 - yesPct;
  const hoverActive =
    state.chartHoverMarketId === market.id && state.chartHoverX !== null;
  const hoverX = hoverActive
    ? Math.max(plotLeft, Math.min(plotRight, state.chartHoverX as number))
    : yesEnd.x;
  const hoverT =
    plotXSpan <= 0
      ? 1
      : Math.max(0, Math.min(1, (hoverX - plotLeft) / plotXSpan));
  const seriesPosition = hoverT * (pointCount - 1);
  const leftIndex = Math.max(
    0,
    Math.min(pointCount - 1, Math.floor(seriesPosition)),
  );
  const rightIndex = Math.max(
    leftIndex,
    Math.min(pointCount - 1, Math.ceil(seriesPosition)),
  );
  const mix = Math.max(0, Math.min(1, seriesPosition - leftIndex));
  const yesHoverValue =
    yesSeries[leftIndex] + (yesSeries[rightIndex] - yesSeries[leftIndex]) * mix;
  const separatedHover = separateSeriesY(
    yFromProbability(yesHoverValue),
    yFromProbability(1 - yesHoverValue),
  );
  const yesHover = {
    x: hoverX,
    y: separatedHover.yesY,
  };
  const noHover = { x: hoverX, y: separatedHover.noY };
  const hoverTime = new Date(
    startTime.getTime() + (now.getTime() - startTime.getTime()) * hoverT,
  );
  const hoverTimeLabel = hoverTime.toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
    timeZone: "America/New_York",
  });
  const hoverYesPct = Math.round(yesHoverValue * 100);
  const hoverNoPct = 100 - hoverYesPct;
  const endpointOpacity = hoverActive ? "0.4" : "1";
  const showCurrentPulse = !hoverActive || hoverT > 0.985;
  const fadeX = hoverX;
  const fadeW = Math.max(0, plotRight - fadeX);
  const pawSkipZones = [
    { x: yesEnd.x, y: yesEnd.y, r: 3.5 },
    { x: noEnd.x, y: noEnd.y, r: 3.5 },
    ...(hoverActive
      ? [
          { x: yesHover.x, y: yesHover.y, r: 3.3 },
          { x: noHover.x, y: noHover.y, r: 3.3 },
        ]
      : []),
  ];
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
  const clampReadoutTop = (y: number): number =>
    Math.max(plotTop + 0.6, Math.min(plotBottom - readoutBlockHeight - 0.6, y));
  const noAnchorY = hoverActive ? noHover.y : noEnd.y;
  const yesAnchorY = hoverActive ? yesHover.y : yesEnd.y;
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
  const readoutNoPct = hoverActive ? hoverNoPct : noPct;
  const readoutYesPct = hoverActive ? hoverYesPct : yesPct;
  const legendNoPct = hoverActive ? hoverNoPct : noPct;
  const legendYesPct = hoverActive ? hoverYesPct : yesPct;
  const hoverTimeX = Math.max(plotLeft + 18, Math.min(plotRight - 18, hoverX));
  const hoverTimeText = `${hoverTimeLabel} ET`;
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

  return `
    <div style="font-variant-numeric: tabular-nums;">
      <div class="relative ${isHomeChart ? "h-[17.5rem]" : "h-[19.5rem]"} rounded-xl border border-slate-800 bg-slate-950/60 p-3">
      <div class="mb-2 flex items-center gap-4 text-[14px] font-medium text-slate-300">
        <span class="inline-flex items-center gap-1 text-slate-200">${legendIcon("#5eead4")}Yes ${legendYesPct}%</span>
        <span class="inline-flex items-center gap-1 text-slate-200">${legendIcon("#fb7185")}No ${legendNoPct}%</span>
        <span class="text-slate-500">Yes + No = ${SATS_PER_FULL_CONTRACT} sats</span>
        ${
          market.isLive
            ? '<span class="inline-flex items-center gap-1 text-[11px] font-semibold text-rose-400"><span class="liveIndicatorDot"></span>Live · Round 1</span>'
            : ""
        }
      </div>
      <div class="pointer-events-none absolute inset-x-3 top-10 bottom-8">
        <svg viewBox="0 0 ${chartWidth} ${chartHeight}" preserveAspectRatio="none" class="h-full w-full">
          ${guideLines}
          <polyline fill="none" stroke="#5eead4" stroke-opacity="0.64" stroke-width="1.08" points="${yesLinePoints}" />
          <polyline fill="none" stroke="#fb7185" stroke-opacity="0.6" stroke-width="1.08" points="${noLinePoints}" />
          ${pawTrail(yesPoints, "#3fbcae", pawSkipZones)}
          ${pawTrail(noPoints, "#e06b7f", pawSkipZones)}
          ${
            hoverActive
              ? `<rect x="${fadeX}" y="${plotTop}" width="${fadeW}" height="${plotYSpan}" fill="#020617" fill-opacity="0.5" />
          <line x1="${yesHover.x}" y1="${plotTop}" x2="${yesHover.x}" y2="${plotBottom}" stroke="#e2e8f0" stroke-opacity="0.6" stroke-width="0.32" />`
              : ""
          }
          ${
            showCurrentPulse
              ? `${pulseAt(yesEnd.x, yesEnd.y, "chartLivePulseYes")}
          ${pulseAt(noEnd.x, noEnd.y, "chartLivePulseNo")}`
              : ""
          }
          <g opacity="${endpointOpacity}">
            ${markerAt(yesEnd.x, yesEnd.y, "#5eead4")}
            ${markerAt(noEnd.x, noEnd.y, "#fb7185")}
          </g>
          ${
            hoverActive
              ? `${markerAt(yesHover.x, yesHover.y, "#5eead4", 1.16)}
          ${markerAt(noHover.x, noHover.y, "#fb7185", 1.16)}`
              : ""
          }
          ${
            hoverActive
              ? `<rect x="${hoverTimeBoxX}" y="${hoverTimeBoxY}" width="${hoverTimeBoxWidth}" height="${hoverTimeBoxHeight}" rx="2.45" fill="#020617" fill-opacity="0.8" stroke="#475569" stroke-opacity="0.56" stroke-width="0.24" />
          <text x="${hoverTimeTextX}" y="${hoverTimeTextY}" fill="#dbe7f6" font-size="${hoverTimeFontSize}" font-weight="430" text-anchor="middle" style="paint-order:stroke;stroke:#020617;stroke-width:${hoverTimeStrokeWidth};stroke-opacity:0.45;">${hoverTimeText}</text>`
              : ""
          }
          <text x="${readoutX}" y="${readoutNoLabelY}" fill="#fda4af" font-size="${readoutLabelFont}" font-weight="520" style="paint-order:stroke;stroke:#020617;stroke-width:${readoutStrokeWidth};stroke-opacity:0.82;">NO</text>
          <text x="${readoutX}" y="${readoutNoPctY}" fill="#f98fa2" font-size="${readoutPctFont}" font-weight="560" style="paint-order:stroke;stroke:#020617;stroke-width:${readoutStrokeWidth};stroke-opacity:0.82;">${readoutNoPct}%</text>
          <text x="${readoutX}" y="${readoutYesLabelY}" fill="#99f6e4" font-size="${readoutLabelFont}" font-weight="520" style="paint-order:stroke;stroke:#020617;stroke-width:${readoutStrokeWidth};stroke-opacity:0.82;">YES</text>
          <text x="${readoutX}" y="${readoutYesPctY}" fill="#84f4cb" font-size="${readoutPctFont}" font-weight="560" style="paint-order:stroke;stroke:#020617;stroke-width:${readoutStrokeWidth};stroke-opacity:0.82;">${readoutYesPct}%</text>
        </svg>
      </div>
      <div
        class="absolute inset-x-3 top-10 bottom-8 z-10"
        data-chart-hover="1"
        data-market-id="${market.id}"
        data-chart-mode="${mode}"
        data-point-count="${pointCount}"
        data-plot-width="${chartWidth}"
        data-plot-left="${plotLeft}"
        data-plot-right="${plotRight}"
      ></div>
      <div class="pointer-events-none absolute right-1 top-10 bottom-8 flex flex-col justify-between text-[12px] font-normal text-slate-500" style="text-shadow: 0 1px 1px rgba(2, 6, 23, 0.35);"><span>100%</span><span>75%</span><span>50%</span><span>25%</span><span>0%</span></div>
      <div class="pointer-events-none absolute inset-x-3 bottom-1 flex items-center justify-between text-[12px] font-normal text-slate-500" style="text-shadow: 0 1px 1px rgba(2, 6, 23, 0.35);">${xLabels
        .map(
          (label, idx) =>
            `<span data-est-label data-offset-hours="${xLabelOffsets[idx]}">${formatEstTime(label)}</span>`,
        )
        .join(
          "",
        )}<span class="ml-2 text-[11px] uppercase tracking-wide text-slate-600">ET</span></div>
      </div>
      <div class="mt-2 flex items-center justify-between">
        <span class="pl-0.5 text-[13px] font-medium text-slate-300">${volumeLabel}</span>
        <div class="inline-flex items-center gap-1 rounded-lg border border-slate-800 bg-slate-950/65 p-1 text-[12px]">
        ${(["1H", "3H", "6H", "12H", "1D"] as const)
          .map(
            (option) =>
              `<button data-action="set-chart-timescale" data-scale="${option}" class="rounded px-2 py-0.5 transition ${state.chartTimescale === option ? "bg-slate-700 text-slate-100" : "text-slate-500 hover:bg-slate-800/70 hover:text-slate-300"}">${option}</button>`,
          )
          .join("")}
        </div>
      </div>
    </div>
  `;
}

export function renderPathCard(
  label: string,
  enabled: boolean,
  formula: string,
  next: string,
): string {
  return `<div class="rounded-lg border px-3 py-2 ${
    enabled
      ? "border-slate-700 bg-slate-900/50 text-slate-300"
      : "border-slate-800 bg-slate-950/60 text-slate-400"
  }">
    <div class="mb-1 flex items-center justify-between gap-2">
      <p class="text-sm font-medium">${label}</p>
      <span class="status-chip ${enabled ? "border border-emerald-500/40 bg-emerald-500/10 text-emerald-300" : "border border-slate-700 bg-slate-800/60 text-slate-400"}">${enabled ? "Available" : "Locked"}</span>
    </div>
    <p class="text-xs">${formula}</p>
    <p class="mt-1 text-xs">${next}</p>
  </div>`;
}

export function renderActionTicket(market: Market): string {
  const paths = getPathAvailability(market);
  const preview = getTradePreview(market);
  const executionPriceSats = Math.round(preview.executionPriceSats);
  const positions = getPositionContracts(market);
  const selectedPositionContracts =
    state.selectedSide === "yes" ? positions.yes : positions.no;
  const ctaVerb = state.tradeIntent === "open" ? "Buy" : "Sell";
  const ctaTarget = state.selectedSide === "yes" ? "Yes" : "No";
  const ctaLabel = `${ctaVerb} ${ctaTarget}`;
  const fillabilityLabel =
    state.orderType === "limit"
      ? preview.fill.filledContracts <= 0
        ? "Resting only (not fillable now)"
        : preview.fill.isPartial
          ? "Partially fillable now"
          : "Fully fillable now"
      : preview.fill.isPartial
        ? "May partially fill"
        : "Expected to fill now";

  const issueCollateral = state.pairsInput * 2 * market.cptSats;
  const cancelCollateral = state.pairsInput * 2 * market.cptSats;
  const redeemRate = paths.redeem
    ? 2 * market.cptSats
    : paths.expiryRedeem
      ? market.cptSats
      : 0;
  const redeemCollateral = state.tokensInput * redeemRate;
  const yesDisplaySats = clampContractPriceSats(
    Math.round(market.yesPrice * SATS_PER_FULL_CONTRACT),
  );
  const noDisplaySats = SATS_PER_FULL_CONTRACT - yesDisplaySats;
  const estimatedExecutionFeeSats = Math.round(
    preview.notionalSats * EXECUTION_FEE_RATE,
  );
  const estimatedGrossPayoutSats = Math.floor(
    preview.requestedContracts * SATS_PER_FULL_CONTRACT,
  );
  const estimatedProfitSats = Math.max(
    0,
    estimatedGrossPayoutSats - preview.notionalSats,
  );
  const estimatedWinFeeSats =
    state.tradeIntent === "open"
      ? Math.round(estimatedProfitSats * WIN_FEE_RATE)
      : 0;
  const estimatedFeesSats = estimatedExecutionFeeSats + estimatedWinFeeSats;
  const estimatedNetIfCorrectSats = Math.max(
    0,
    estimatedGrossPayoutSats - estimatedFeesSats,
  );
  return `
    <aside class="rounded-[21px] border border-slate-800 bg-slate-900/80 p-[21px]">
      <p class="panel-subtitle">Contract Action Ticket</p>
      <p class="mb-3 mt-1 text-sm text-slate-300">Buy or sell with a cleaner ticket flow. Advanced covenant actions are below.</p>
      <div class="mb-3 flex items-center justify-between gap-3 border-b border-slate-800 pb-3">
        <div class="flex items-center gap-4">
          <button data-trade-intent="open" class="border-b-2 pb-1 text-xl font-medium ${state.tradeIntent === "open" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Buy</button>
          <button data-trade-intent="close" class="border-b-2 pb-1 text-xl font-medium ${state.tradeIntent === "close" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Sell</button>
        </div>
        <div class="flex items-center gap-2 rounded-lg border border-slate-700 p-1">
          <button data-order-type="market" class="rounded px-3 py-1 text-sm ${state.orderType === "market" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Market</button>
          <button data-order-type="limit" class="rounded px-3 py-1 text-sm ${state.orderType === "limit" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Limit</button>
        </div>
      </div>
      <div class="mb-3 grid grid-cols-2 gap-2">
        <button data-side="yes" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "yes" ? (state.tradeIntent === "open" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-400 bg-slate-400/15 text-slate-200") : "border-slate-700 text-slate-300"}">Yes ${yesDisplaySats} sats</button>
        <button data-side="no" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "no" ? (state.tradeIntent === "open" ? "border-rose-400 bg-rose-400/20 text-rose-200" : "border-slate-400 bg-slate-400/15 text-slate-200") : "border-slate-700 text-slate-300"}">No ${noDisplaySats} sats</button>
      </div>
      <div class="mb-3 flex items-center justify-between gap-2">
        <label class="text-xs text-slate-400">Amount</label>
        <div class="grid grid-cols-2 gap-2">
          <button data-size-mode="sats" class="rounded border px-2 py-1 text-xs ${state.sizeMode === "sats" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">sats</button>
          <button data-size-mode="contracts" class="rounded border px-2 py-1 text-xs ${state.sizeMode === "contracts" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">contracts</button>
        </div>
      </div>
      ${
        state.sizeMode === "sats"
          ? `
      <input id="trade-size-sats" type="text" inputmode="numeric" value="${state.tradeSizeSatsDraft}" class="mb-2 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
      `
          : `
      <div class="mb-3 grid grid-cols-[42px_1fr_42px] gap-2">
        <button data-action="step-trade-contracts" data-contracts-step-delta="-1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Decrease contracts">&minus;</button>
        <input id="trade-size-contracts" type="text" inputmode="decimal" value="${state.tradeContractsDraft}" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <button data-action="step-trade-contracts" data-contracts-step-delta="1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Increase contracts">+</button>
      </div>
      ${
        state.tradeIntent === "close"
          ? `<div class="mb-3 flex items-center gap-2 text-sm">
        <button data-action="sell-25" class="rounded border border-slate-700 px-3 py-1 text-slate-300">25%</button>
        <button data-action="sell-50" class="rounded border border-slate-700 px-3 py-1 text-slate-300">50%</button>
        <button data-action="sell-max" class="rounded border border-slate-700 px-3 py-1 text-slate-300">Max</button>
      </div>`
          : ""
      }
      `
      }
      ${
        state.orderType === "limit"
          ? `
      <label for="limit-price" class="mb-1 block text-xs text-slate-400">Limit price (sats)</label>
      <div class="mb-3 grid grid-cols-[42px_1fr_42px] gap-2">
        <button data-action="step-limit-price" data-limit-price-delta="-1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Decrease limit price">&minus;</button>
        <input id="limit-price" type="text" inputmode="numeric" pattern="[0-9]*" maxlength="2" value="${state.limitPriceDraft}" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <button data-action="step-limit-price" data-limit-price-delta="1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Increase limit price">+</button>
      </div>
      <p class="mb-3 text-xs text-slate-500">May not fill immediately; unfilled size rests on book. ${fillabilityLabel}. Matchable now: ${formatSats(preview.executedSats)}.</p>
      `
          : `<p class="mb-3 text-xs text-slate-500">Estimated avg fill: ${preview.fill.avgPriceSats.toFixed(1)} sats (range ${preview.fill.bestPriceSats}-${preview.fill.worstPriceSats}).</p>`
      }
      <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
        ${
          state.tradeIntent === "open"
            ? `<div class="flex items-center justify-between py-1"><span>You pay</span><span>${formatSats(preview.notionalSats)}</span></div>
        <div class="flex items-center justify-between py-1"><span>If filled & correct</span><span>${formatSats(estimatedNetIfCorrectSats)}</span></div>`
            : `<div class="flex items-center justify-between py-1"><span>You receive (if filled)</span><span>${formatSats(Math.max(0, preview.notionalSats - estimatedExecutionFeeSats))}</span></div>
        <div class="flex items-center justify-between py-1"><span>Position remaining (if filled)</span><span>${Math.max(0, selectedPositionContracts - preview.requestedContracts).toFixed(2)} contracts</span></div>`
        }
        <div class="flex items-center justify-between py-1"><span>Estimated fees</span><span>${formatSats(estimatedFeesSats)}</span></div>
        <div class="mt-1 flex items-center justify-between py-1 text-xs text-slate-500"><span>Price</span><span>${executionPriceSats} sats · Yes + No = ${SATS_PER_FULL_CONTRACT}</span></div>
      </div>
      <button data-action="submit-trade" class="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950">${ctaLabel}</button>
      <div class="mt-3 flex items-center justify-between text-xs text-slate-400">
        <span>You hold: YES ${positions.yes.toFixed(2)} · NO ${positions.no.toFixed(2)}</span>
        ${
          state.tradeIntent === "close"
            ? `<button data-action="sell-max" class="rounded border border-slate-700 px-2 py-1 text-slate-300">Sell max</button>`
            : ""
        }
      </div>
      ${
        state.tradeIntent === "close" &&
        preview.requestedContracts > selectedPositionContracts + 0.0001
          ? `<p class="mt-2 text-xs text-rose-300">Requested size exceeds your current ${state.selectedSide.toUpperCase()} position.</p>`
          : ""
      }
      <div class="mt-3 flex items-center gap-2">
        <button data-action="toggle-orderbook" class="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-300">${state.showOrderbook ? "Hide depth" : "Show depth"}</button>
        <button data-action="toggle-fee-details" class="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-300">${state.showFeeDetails ? "Hide fee details" : "Fee details"}</button>
      </div>
      ${
        state.showOrderbook
          ? `<div class="mt-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-xs">
        <p class="mb-2 font-semibold text-slate-200">${state.tradeIntent === "open" ? "Asks (buy depth)" : "Bids (sell depth)"} · ${state.selectedSide.toUpperCase()}</p>
        <div class="space-y-1">
          ${getOrderbookLevels(market, state.selectedSide, state.tradeIntent)
            .map(
              (
                level,
                idx,
              ) => `<div class="flex items-center justify-between rounded ${idx === 0 ? "bg-slate-900/70" : ""} px-2 py-1">
            <span>${level.priceSats} sats</span>
            <span>${level.contracts.toFixed(2)} contracts</span>
          </div>`,
            )
            .join("")}
        </div>
      </div>`
          : ""
      }
      ${
        state.showFeeDetails
          ? `<div class="mt-3 rounded border border-slate-800 bg-slate-900/40 p-2 text-xs text-slate-400">
        <p>Execution fee: 1% of matched notional.</p>
        <p>Winning PnL fee: 2% of positive payout minus entry cost (buy only).</p>
        <p>Final fee depends on actual matched fills.</p>
      </div>`
          : ""
      }
      <section class="mt-4 rounded-xl border border-slate-800 bg-slate-950/50 p-3">
        <div class="flex items-center justify-between">
          <p class="text-xs text-slate-500">Advanced actions</p>
          <button data-action="toggle-advanced-actions" class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-300">${state.showAdvancedActions ? "Hide" : "Show"}</button>
        </div>
      </section>
      ${
        state.showAdvancedActions
          ? `
      <div class="mt-3 grid grid-cols-3 gap-2">
        <button data-tab="issue" class="rounded border px-3 py-2 text-sm ${state.actionTab === "issue" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Issue</button>
        <button data-tab="redeem" class="rounded border px-3 py-2 text-sm ${state.actionTab === "redeem" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Redeem</button>
        <button data-tab="cancel" class="rounded border px-3 py-2 text-sm ${state.actionTab === "cancel" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Cancel</button>
      </div>
      ${
        state.actionTab === "issue"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.initialIssue ? "0 \u2192 1 Initial Issuance" : "1 \u2192 1 Subsequent Issuance"}</p>
        <label for="pairs-input" class="mb-1 block text-xs text-slate-400">Pairs to mint</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Required collateral</span><span>${formatSats(issueCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT (${state.pairsInput} * 2 * ${market.cptSats})</div>
        </div>
        <button data-action="submit-issue" ${paths.issue || paths.initialIssue ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.issue || paths.initialIssue ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit Issuance Transaction</button>
      </div>
      `
          : ""
      }

      ${
        state.actionTab === "redeem"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.redeem ? "Post-resolution redemption" : paths.expiryRedeem ? "Expiry redemption" : "Unavailable"}</p>
        <label for="tokens-input" class="mb-1 block text-xs text-slate-400">Tokens to burn</label>
        <input id="tokens-input" type="number" min="1" step="1" value="${state.tokensInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral withdrawn</span><span>${formatSats(redeemCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: tokens * ${paths.redeem ? "2*CPT" : paths.expiryRedeem ? "CPT" : "N/A"}</div>
        </div>
        <button data-action="submit-redeem" ${paths.redeem || paths.expiryRedeem ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.redeem || paths.expiryRedeem ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit redemption tx</button>
      </div>
      `
          : ""
      }

      ${
        state.actionTab === "cancel"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: 1 \u2192 1 Cancellation</p>
        <label for="pairs-input" class="mb-1 block text-xs text-slate-400">Matched YES/NO pairs to burn</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral refund</span><span>${formatSats(cancelCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT</div>
        </div>
        <button data-action="submit-cancel" ${paths.cancel ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.cancel ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit cancellation tx</button>
      </div>
      `
          : ""
      }
      `
          : ""
      }
    </aside>
  `;
}

export function renderDetail(): string {
  const market = getSelectedMarket();
  const noPrice = 1 - market.yesPrice;
  const paths = getPathAvailability(market);
  const expired = isExpired(market);
  const estimatedSettlementDate = getEstimatedSettlementDate(market);
  const collateralPoolSats = market.collateralUtxos.reduce(
    (sum, utxo) => sum + utxo.amountSats,
    0,
  );

  return `
    <div class="phi-container py-6 lg:py-8">
      ${
        expired && market.state === 1
          ? `<div class="mb-4 rounded-xl border border-slate-600 bg-slate-900/60 px-4 py-3 text-sm text-slate-300">Market expired unresolved at height ${market.expiryHeight}. Expiry redemption path is active. Issuance and oracle resolve are disabled.</div>`
          : ""
      }
      <div class="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="space-y-[21px]">
          <div class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[34px]">
            <button data-action="go-home" class="mb-3 flex items-center gap-1 text-sm text-slate-400 transition hover:text-slate-200">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
              Markets
            </button>
            <div class="mb-2 flex items-center gap-2">
              <span class="rounded-full bg-slate-800 px-2.5 py-0.5 text-xs text-slate-300">${market.category}</span>
              ${stateBadge(market.state)}
              ${market.creationTxid ? `<button data-action="refresh-market-state" class="rounded p-0.5 text-slate-500 transition hover:text-slate-300" title="Refresh state"><svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg></button>` : ""}
            </div>
            <div class="mb-3 flex items-center gap-2">
              <button data-action="open-nostr-event" data-market-id="${market.id}" data-nevent="${market.nevent}" class="rounded-lg border border-slate-700 px-3 py-1 text-xs text-slate-300 transition hover:bg-slate-800 hover:text-slate-200">Nostr Event</button>
              ${market.creationTxid ? `<button data-action="open-explorer-tx" data-txid="${market.creationTxid}" class="rounded-lg border border-slate-700 px-3 py-1 text-xs text-slate-300 transition hover:bg-slate-800 hover:text-slate-200">Creation TX</button>` : ""}
            </div>
            <h1 class="phi-title mb-2 text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">${market.question}</h1>
            <p class="mb-3 text-base text-slate-400">${market.description}</p>

            <div class="mb-4 grid gap-3 sm:grid-cols-3">
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Yes price<br/><span class="text-lg font-medium text-emerald-400">${formatProbabilityWithPercent(market.yesPrice)}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">No price<br/><span class="text-lg font-medium text-rose-400">${formatProbabilityWithPercent(noPrice)}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Settlement deadline<br/><span class="text-slate-100">Est. by ${formatSettlementDateTime(estimatedSettlementDate)}</span></div>
            </div>

            ${(() => {
              const pos = getPositionContracts(market);
              if (pos.yes === 0 && pos.no === 0) return "";
              return `<div class="mb-4 flex items-center gap-3 rounded-xl border border-slate-700 bg-slate-900/40 px-4 py-3 text-sm">
                <span class="text-slate-400">Your position</span>
                ${pos.yes > 0 ? `<span class="rounded bg-emerald-500/20 px-2 py-0.5 font-medium text-emerald-300">YES ${pos.yes.toLocaleString()}</span>` : ""}
                ${pos.no > 0 ? `<span class="rounded bg-red-500/20 px-2 py-0.5 font-medium text-red-300">NO ${pos.no.toLocaleString()}</span>` : ""}
              </div>`;
            })()}

            ${chartSkeleton(market, "detail")}
          </div>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 px-[21px] py-3">
            <div class="flex items-center justify-between gap-4">
              <p class="text-sm text-slate-400"><span class="text-slate-200">Protocol Details</span> — oracle, covenant paths, and collateral mechanics</p>
              <button data-action="toggle-advanced-details" class="shrink-0 rounded-lg border border-slate-700 px-3 py-1.5 text-sm text-slate-200">${state.showAdvancedDetails ? "Hide" : "Show"}</button>
            </div>
          </section>

          ${
            state.showAdvancedDetails
              ? `
          <div class="grid gap-3 lg:grid-cols-2">
            <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
              <p class="panel-subtitle">Oracle</p>
              <h3 class="panel-title mb-2 text-lg">Oracle Attestation</h3>
              <div class="space-y-1 text-xs text-slate-300">
                <div class="kv-row"><span class="shrink-0">Oracle</span><button data-action="copy-to-clipboard" data-copy-value="${hexToNpub(market.oraclePubkey)}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${hexToNpub(market.oraclePubkey)}">${(() => {
                  const n = hexToNpub(market.oraclePubkey);
                  return `${n.slice(0, 10)}...${n.slice(-6)}`;
                })()}</button></div>
                <div class="kv-row"><span class="shrink-0">Market ID</span><button data-action="copy-to-clipboard" data-copy-value="${market.marketId}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${market.marketId}">${market.marketId.slice(0, 8)}...${market.marketId.slice(-8)}</button></div>
                <div class="kv-row"><span class="shrink-0">Block target</span><span class="mono">${formatBlockHeight(market.expiryHeight)}</span></div>
                <div class="kv-row"><span class="shrink-0">Current height</span><span class="mono">${formatBlockHeight(market.currentHeight)}</span></div>
                <div class="kv-row"><span class="shrink-0">Message domain</span><span class="mono text-right">SHA256(ID || outcome)</span></div>
                <div class="kv-row"><span class="shrink-0">Outcome bytes</span><span class="mono">YES=0x01, NO=0x00</span></div>
                <div class="kv-row"><span class="shrink-0">Resolve status</span><span class="${market.resolveTx?.sigVerified ? "text-emerald-300" : "text-slate-400"}">${market.resolveTx ? `Attested ${market.resolveTx.outcome.toUpperCase()} @ ${market.resolveTx.height}` : "Unresolved"}</span></div>
                ${market.resolveTx ? `<div class="kv-row"><span class="shrink-0">Sig hash</span><button data-action="copy-to-clipboard" data-copy-value="${market.resolveTx.signatureHash}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${market.resolveTx.signatureHash}">${market.resolveTx.signatureHash.slice(0, 8)}...${market.resolveTx.signatureHash.slice(-8)}</button></div><div class="kv-row"><span class="shrink-0">Resolve tx</span><button data-action="copy-to-clipboard" data-copy-value="${market.resolveTx.txid}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${market.resolveTx.txid}">${market.resolveTx.txid.slice(0, 8)}...${market.resolveTx.txid.slice(-8)}</button></div>` : ""}
              </div>
              ${
                state.nostrPubkey &&
                state.nostrPubkey === market.oraclePubkey &&
                market.state === 1 &&
                !market.resolveTx
                  ? `
              <div class="mt-3 rounded-lg border border-amber-700/60 bg-amber-950/20 p-3">
                <p class="mb-2 text-sm font-semibold text-amber-200">You are the oracle for this market</p>
                <div class="flex items-center gap-2">
                  <button data-action="oracle-attest-yes" class="rounded-lg bg-emerald-300 px-4 py-2 text-sm font-semibold text-slate-950">Resolve YES</button>
                  <button data-action="oracle-attest-no" class="rounded-lg bg-rose-400 px-4 py-2 text-sm font-semibold text-slate-950">Resolve NO</button>
                </div>
              </div>`
                  : ""
              }
              ${
                state.lastAttestationSig &&
                state.lastAttestationMarketId === market.marketId &&
                market.state === 1
                  ? `
              <div class="mt-3 rounded-lg border border-emerald-700/60 bg-emerald-950/20 p-3">
                <p class="mb-2 text-sm font-semibold text-emerald-200">Attestation published — execute on-chain resolution</p>
                <p class="mb-2 text-xs text-slate-300">Outcome: ${state.lastAttestationOutcome ? "YES" : "NO"} | Sig: ${state.lastAttestationSig.slice(0, 24)}...</p>
                <button data-action="execute-resolution" ${state.resolutionExecuting ? "disabled" : ""} class="w-full rounded-lg ${state.resolutionExecuting ? "bg-slate-700 text-slate-400" : "bg-emerald-300 text-slate-950"} px-4 py-2 text-sm font-semibold">${state.resolutionExecuting ? "Executing..." : "Execute Resolution On-Chain"}</button>
              </div>`
                  : ""
              }
            </section>

            <section class="rounded-[21px] border ${market.collateralUtxos.length === 1 ? "border-emerald-800" : "border-rose-800"} bg-slate-950/55 p-[21px]">
              <p class="panel-subtitle">Integrity</p>
              <h3 class="panel-title mb-2 text-lg">Single-UTXO Integrity</h3>
              <p class="text-sm ${market.collateralUtxos.length === 1 ? "text-emerald-300" : "text-rose-300"}">${market.collateralUtxos.length === 1 ? "OK: exactly one collateral UTXO" : "ALERT: fragmented collateral UTXO set"}</p>
              <div class="mt-2 space-y-1 text-xs text-slate-300">
                ${market.collateralUtxos
                  .map(
                    (utxo) =>
                      `<div class="kv-row"><button data-action="copy-to-clipboard" data-copy-value="${utxo.txid}:${utxo.vout}" class="mono truncate hover:text-slate-100 transition cursor-pointer" title="${utxo.txid}:${utxo.vout}">${utxo.txid.slice(0, 8)}...${utxo.txid.slice(-8)}:${utxo.vout}</button><span class="mono shrink-0">${formatSats(utxo.amountSats)}</span></div>`,
                  )
                  .join("")}
              </div>
              <p class="mt-2 text-xs text-slate-500">Collateral pool: ${formatSats(collateralPoolSats)} · ${stateLabel(market.state)}</p>
            </section>
          </div>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <p class="panel-subtitle">Accounting</p>
            <h3 class="panel-title mb-2 text-lg">Collateral Mechanics</h3>
            <div class="grid gap-2 md:grid-cols-2 text-sm">
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Issuance: <span class="text-slate-100">pairs * 2 * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Post-resolution redeem: <span class="text-slate-100">tokens * 2 * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Expiry redeem: <span class="text-slate-100">tokens * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Cancellation: <span class="text-slate-100">pairs * 2 * CPT</span></div>
            </div>
          </section>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <p class="panel-subtitle">State Machine</p>
            <h3 class="panel-title mb-2 text-lg">Covenant Paths</h3>
            <div class="grid gap-2 md:grid-cols-2">
              ${renderPathCard("0 \u2192 1 Initial issuance", paths.initialIssue, "pairs * 2 * CPT", "Outputs move to state-1 address")}
              ${renderPathCard("1 \u2192 1 Subsequent issuance", paths.issue, "pairs * 2 * CPT", "Collateral UTXO reconsolidated")}
              ${renderPathCard("1 \u2192 2/3 Oracle resolve", paths.resolve, "state commit via oracle signature", "All covenant outputs move atomically")}
              ${renderPathCard("2/3 Redemption", paths.redeem, "tokens * 2 * CPT", "Winning side burns tokens")}
              ${renderPathCard("1 Expiry redemption", paths.expiryRedeem, "tokens * CPT", "Unresolved + expiry only")}
              ${renderPathCard("1 \u2192 1 Cancellation", paths.cancel, "pairs * 2 * CPT", "Equal YES/NO burn")}
            </div>
          </section>
          `
              : ""
          }
        </section>

        ${renderActionTicket(market)}
      </div>
    </div>
  `;
}
