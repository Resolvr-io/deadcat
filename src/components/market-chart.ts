import { SATS_PER_FULL_CONTRACT, state } from "../state.ts";
import type { Market } from "../types.ts";
import { formatEstTime } from "../utils/format.ts";
import { buildChartSeriesData } from "./chart-series.ts";

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

  const yes = market.yesPrice ?? 0.5;
  const { endTime, pointCount, startTime, xLabelOffsets, xLabels, yesSeries } =
    buildChartSeriesData(market);
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
    startTime.getTime() + (endTime.getTime() - startTime.getTime()) * hoverT,
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
