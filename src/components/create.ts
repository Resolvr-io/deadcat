import { categories, state } from "../state.ts";
import { escapeAttr, escapeHtml } from "../utils/html.ts";

const chevronSvg =
  '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0 text-slate-400"><polyline points="6 9 12 15 18 9"/></svg>';

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** Renders a mini custom dropdown matching the category dropdown style. */
function miniDropdown(
  name: string,
  label: string,
  items: { value: string; label: string; selected: boolean }[],
): string {
  const isOpen = state.createSettlementPickerDropdown === name;
  const menuItems = items
    .map(
      (it) =>
        `<button type="button" data-action="pick-settlement-option" data-dropdown="${escapeAttr(name)}" data-value="${escapeAttr(it.value)}" class="dc-dropdown-option ${it.selected ? "dc-dropdown-option-active" : ""}">${escapeHtml(it.label)}</button>`,
    )
    .join("");

  return `
    <div class="relative">
      <button type="button" data-action="toggle-settlement-dropdown" data-dropdown="${escapeAttr(name)}" class="dc-dropdown-trigger">
        <span>${escapeHtml(label)}</span>
        ${chevronSvg}
      </button>
      ${isOpen ? `<div class="dc-dropdown-menu">${menuItems}</div>` : ""}
    </div>`;
}

function renderCalendar(): string {
  const viewYear = state.createSettlementViewYear;
  const viewMonth = state.createSettlementViewMonth;

  const selected = state.createSettlementInput
    ? new Date(state.createSettlementInput)
    : null;
  const selectedDay =
    selected &&
    selected.getFullYear() === viewYear &&
    selected.getMonth() === viewMonth
      ? selected.getDate()
      : -1;

  const today = new Date();
  const todayDay =
    today.getFullYear() === viewYear && today.getMonth() === viewMonth
      ? today.getDate()
      : -1;

  const firstDay = new Date(viewYear, viewMonth, 1).getDay();
  const daysInMonth = new Date(viewYear, viewMonth + 1, 0).getDate();

  // Time
  let hour12 = 12;
  let minute = 0;
  let isPM = true;
  if (selected) {
    const h = selected.getHours();
    isPM = h >= 12;
    hour12 = h % 12 || 12;
    minute = Math.floor(selected.getMinutes() / 5) * 5;
  }

  // Day cells
  const dayCells: string[] = [];
  for (let i = 0; i < firstDay; i++) {
    dayCells.push('<div class="h-9 w-9"></div>');
  }
  for (let d = 1; d <= daysInMonth; d++) {
    const isSelected = d === selectedDay;
    const isToday = d === todayDay;
    let cls =
      "h-9 w-9 rounded-lg text-sm transition-colors cursor-pointer flex items-center justify-center";
    if (isSelected) {
      cls += " bg-emerald-400 text-slate-950 font-semibold";
    } else if (isToday) {
      cls += " ring-1 ring-emerald-400/40 text-slate-100 hover:bg-slate-700/60";
    } else {
      cls += " text-slate-300 hover:bg-slate-700/60";
    }
    dayCells.push(
      `<button type="button" data-action="pick-settlement-day" data-day="${d}" class="${cls}">${d}</button>`,
    );
  }

  // Build dropdown data
  const thisYear = new Date().getFullYear();
  const monthDropdown = miniDropdown(
    "month",
    MONTHS[viewMonth],
    MONTHS.map((m, i) => ({
      value: String(i),
      label: m,
      selected: i === viewMonth,
    })),
  );
  const yearDropdown = miniDropdown(
    "year",
    String(viewYear),
    Array.from({ length: 11 }, (_, i) => {
      const y = thisYear + i;
      return { value: String(y), label: String(y), selected: y === viewYear };
    }),
  );
  const hourDropdown = miniDropdown(
    "hour",
    String(hour12),
    Array.from({ length: 12 }, (_, i) => {
      const h = i + 1;
      return { value: String(h), label: String(h), selected: h === hour12 };
    }),
  );
  const minuteDropdown = miniDropdown(
    "minute",
    String(minute).padStart(2, "0"),
    Array.from({ length: 12 }, (_, i) => {
      const m = i * 5;
      return {
        value: String(m),
        label: String(m).padStart(2, "0"),
        selected: m === minute,
      };
    }),
  );
  const ampmDropdown = miniDropdown("ampm", isPM ? "PM" : "AM", [
    { value: "AM", label: "AM", selected: !isPM },
    { value: "PM", label: "PM", selected: isPM },
  ]);

  return `
    <div class="absolute top-full left-0 z-50 mt-1 w-[300px] rounded-xl border border-slate-700 bg-slate-900 p-4 shadow-[0_12px_32px_rgba(0,0,0,0.5)]" id="settlement-picker-popover">
      <div class="mb-3 flex items-center justify-between">
        <button type="button" data-action="settlement-prev-month" class="flex h-8 w-8 items-center justify-center rounded-lg text-slate-400 transition-colors hover:bg-slate-800 hover:text-slate-200">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
        </button>
        <div class="flex items-center gap-1.5">
          ${monthDropdown}
          ${yearDropdown}
        </div>
        <button type="button" data-action="settlement-next-month" class="flex h-8 w-8 items-center justify-center rounded-lg text-slate-400 transition-colors hover:bg-slate-800 hover:text-slate-200">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
        </button>
      </div>

      <div class="mb-1 grid grid-cols-7 text-center">
        ${["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"].map((d) => `<div class="flex h-9 w-9 items-center justify-center text-[11px] font-medium uppercase text-slate-500">${d}</div>`).join("")}
      </div>

      <div class="grid grid-cols-7 text-center">
        ${dayCells.join("")}
      </div>

      <div class="mt-3 flex items-center gap-2 border-t border-slate-700/60 pt-3">
        <span class="text-xs text-slate-400">Time</span>
        ${hourDropdown}
        <span class="text-slate-500">:</span>
        ${minuteDropdown}
        ${ampmDropdown}
      </div>
    </div>
  `;
}

export function renderCreateMarket(): string {
  const settlementDisplay = state.createSettlementInput
    ? new Date(state.createSettlementInput).toLocaleString("en-US", {
        weekday: "short",
        month: "short",
        day: "numeric",
        year: "numeric",
        hour: "numeric",
        minute: "2-digit",
      })
    : "Select date and time";

  const filteredCategories = categories.filter((item) => item !== "Trending");

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="mx-auto grid max-w-[1180px] gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[34px]">
          <button data-action="cancel-create-market" class="mb-3 flex items-center gap-1 text-sm text-slate-400 transition hover:text-slate-200">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
            Markets
          </button>
          <div class="mb-5">
            <p class="panel-subtitle">Prediction Contract</p>
            <h1 class="phi-title text-xl font-medium text-slate-100 lg:text-2xl">Create New Market</h1>
          </div>

          <div class="space-y-4">
            <div>
              <label for="create-question" class="mb-1 block text-xs text-slate-400">Question</label>
              <input id="create-question" value="${escapeAttr(state.createQuestion)}" maxlength="140" class="dc-input" placeholder="Will X happen by Y?" />
            </div>

            <div>
              <label for="create-description" class="mb-1 block text-xs text-slate-400">Settlement rule</label>
              <textarea id="create-description" rows="3" maxlength="280" class="dc-input h-auto" placeholder="Define exactly how YES/NO resolves.">${escapeHtml(state.createDescription)}</textarea>
            </div>

            <div class="grid gap-4 md:grid-cols-2">
              <div>
                <label class="mb-1 block text-xs text-slate-400">Category</label>
                <div class="relative" id="create-category-dropdown">
                  <button type="button" data-action="toggle-category-dropdown" class="dc-input flex items-center justify-between gap-2 cursor-pointer text-left">
                    <span>${escapeHtml(state.createCategory)}</span>
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0 text-slate-400 transition-transform ${state.createCategoryOpen ? "rotate-180" : ""}"><polyline points="6 9 12 15 18 9"/></svg>
                  </button>
                  ${
                    state.createCategoryOpen
                      ? `<div class="dc-dropdown-menu right-0">
                    ${filteredCategories.map((item) => `<button type="button" data-action="select-create-category" data-value="${escapeAttr(item)}" class="dc-dropdown-option ${state.createCategory === item ? "dc-dropdown-option-active" : ""}">${escapeHtml(item)}</button>`).join("")}
                  </div>`
                      : ""
                  }
                </div>
              </div>

              <div>
                <label class="mb-1 block text-xs text-slate-400">Settlement deadline</label>
                <div class="relative" id="settlement-picker">
                  <button type="button" data-action="toggle-settlement-picker" class="dc-input flex items-center gap-2 cursor-pointer text-left">
                    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0 text-slate-400"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>
                    <span class="${state.createSettlementInput ? "text-slate-100" : "text-slate-500"}">${escapeHtml(settlementDisplay)}</span>
                  </button>
                  ${state.createSettlementPickerOpen ? renderCalendar() : ""}
                </div>
              </div>
            </div>

            <div>
              <label for="create-resolution-source" class="mb-1 block text-xs text-slate-400">Resolution source</label>
              <input id="create-resolution-source" value="${escapeAttr(state.createResolutionSource)}" maxlength="120" class="dc-input" placeholder="Official source (e.g., NHC advisory, FEC filing, exchange index)" />
            </div>

          </div>
        </section>

        <aside class="rounded-[21px] border border-slate-800 bg-slate-900/80 p-[21px]">
          <p class="panel-subtitle">Preview</p>
          <h3 class="panel-title mb-3 text-lg">New Contract Ticket</h3>
          <div class="space-y-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3">
            <p class="text-sm text-slate-200">${escapeHtml(state.createQuestion.trim() || "Your market question will appear here.")}</p>
            <p class="text-xs text-slate-400">${escapeHtml(state.createDescription.trim() || "Settlement rule summary will appear here.")}</p>
            <p class="text-xs text-slate-400">Category: <span class="text-slate-200">${escapeHtml(state.createCategory)}</span></p>
            <p class="text-xs text-slate-400">Settlement deadline: <span class="text-slate-200">${escapeHtml(state.createSettlementInput ? settlementDisplay : "Not set")}</span></p>
            <p class="text-xs text-slate-400">Resolution source: <span class="text-slate-200">${escapeHtml(state.createResolutionSource.trim() || "Not set")}</span></p>
          </div>
          <button data-action="submit-create-market" class="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950 disabled:opacity-50" ${state.marketCreating ? "disabled" : ""}>${state.marketCreating ? "Creating Market..." : "Create Market"}</button>
          <p class="mt-2 text-xs text-slate-500">${state.marketCreating ? "Building transaction, broadcasting, and announcing. This may take a moment." : "Creates the on-chain contract and announces the market. Your key is the oracle signing key."}</p>
        </aside>
      </div>
    </div>
  `;
}
