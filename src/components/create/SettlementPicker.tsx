import { useCallback, useEffect, useRef } from "react";
import { useStore } from "../../store";

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

const DAY_NAMES = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

// ── Mini dropdown ───────────────────────────────────────────────────
function MiniDropdown({
  name,
  label,
  items,
}: {
  name: string;
  label: string;
  items: { value: string; label: string; selected: boolean }[];
}) {
  const openDropdown = useStore((s) => s.createSettlementPickerDropdown);
  const isOpen = openDropdown === name;

  const toggleOpen = useCallback(() => {
    useStore.setState((s) => ({
      createSettlementPickerDropdown:
        s.createSettlementPickerDropdown === name ? "" : name,
    }));
  }, [name]);

  const pickOption = useCallback(
    (value: string) => {
      useStore.setState({ createSettlementPickerDropdown: "" });

      if (name === "month") {
        useStore.setState({ createSettlementViewMonth: Number(value) });
      } else if (name === "year") {
        useStore.setState({ createSettlementViewYear: Number(value) });
      } else if (
        (name === "hour" || name === "minute" || name === "ampm") &&
        useStore.getState().createSettlementInput
      ) {
        const prev = new Date(useStore.getState().createSettlementInput);
        let h = prev.getHours();
        let min = prev.getMinutes();
        const wasPM = h >= 12;
        let h12 = h % 12 || 12;

        if (name === "hour") h12 = Number(value);
        if (name === "minute") min = Number(value);
        let pm = wasPM;
        if (name === "ampm") pm = value === "PM";

        h = (h12 % 12) + (pm ? 12 : 0);

        const y = prev.getFullYear();
        const mo = String(prev.getMonth() + 1).padStart(2, "0");
        const d = String(prev.getDate()).padStart(2, "0");
        const hh = String(h).padStart(2, "0");
        const mm = String(min).padStart(2, "0");
        useStore.setState({
          createSettlementInput: `${y}-${mo}-${d}T${hh}:${mm}`,
        });
      }
    },
    [name],
  );

  return (
    <div className="relative">
      <button
        type="button"
        onClick={toggleOpen}
        className="dc-dropdown-trigger"
      >
        <span>{label}</span>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
          className="shrink-0 text-slate-400"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>
      {isOpen && (
        <div className="dc-dropdown-menu">
          {items.map((it) => (
            <button
              key={it.value}
              type="button"
              onClick={() => pickOption(it.value)}
              className={`dc-dropdown-option ${it.selected ? "dc-dropdown-option-active" : ""}`}
            >
              {it.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Main settlement picker ──────────────────────────────────────────
export default function SettlementPicker() {
  const settlementInput = useStore((s) => s.createSettlementInput);
  const pickerOpen = useStore((s) => s.createSettlementPickerOpen);
  const viewYear = useStore((s) => s.createSettlementViewYear);
  const viewMonth = useStore((s) => s.createSettlementViewMonth);
  const pickerRef = useRef<HTMLDivElement>(null);

  const settlementDisplay = settlementInput
    ? new Date(settlementInput).toLocaleString("en-US", {
        weekday: "short",
        month: "short",
        day: "numeric",
        year: "numeric",
        hour: "numeric",
        minute: "2-digit",
      })
    : "Select date and time";

  const togglePicker = useCallback(() => {
    const state = useStore.getState();
    const nextOpen = !state.createSettlementPickerOpen;
    const updates: Record<string, unknown> = {
      createSettlementPickerOpen: nextOpen,
    };
    if (nextOpen && state.createSettlementInput) {
      const d = new Date(state.createSettlementInput);
      updates.createSettlementViewYear = d.getFullYear();
      updates.createSettlementViewMonth = d.getMonth();
    }
    useStore.setState(updates);
  }, []);

  // Close on outside click
  useEffect(() => {
    if (!pickerOpen) return;
    function handleClickOutside(e: MouseEvent) {
      if (
        pickerRef.current &&
        !pickerRef.current.contains(e.target as Node)
      ) {
        useStore.setState({
          createSettlementPickerOpen: false,
          createSettlementPickerDropdown: "",
        });
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [pickerOpen]);

  const prevMonth = useCallback(() => {
    useStore.setState((s) => {
      let m = s.createSettlementViewMonth - 1;
      let y = s.createSettlementViewYear;
      if (m < 0) {
        m = 11;
        y--;
      }
      return { createSettlementViewMonth: m, createSettlementViewYear: y };
    });
  }, []);

  const nextMonth = useCallback(() => {
    useStore.setState((s) => {
      let m = s.createSettlementViewMonth + 1;
      let y = s.createSettlementViewYear;
      if (m > 11) {
        m = 0;
        y++;
      }
      return { createSettlementViewMonth: m, createSettlementViewYear: y };
    });
  }, []);

  const pickDay = useCallback(
    (day: number) => {
      let hours = 12;
      let minutes = 0;
      if (settlementInput) {
        const prev = new Date(settlementInput);
        hours = prev.getHours();
        minutes = prev.getMinutes();
      }
      const y = viewYear;
      const m = String(viewMonth + 1).padStart(2, "0");
      const d = String(day).padStart(2, "0");
      const hh = String(hours).padStart(2, "0");
      const mm = String(minutes).padStart(2, "0");
      useStore.setState({
        createSettlementInput: `${y}-${m}-${d}T${hh}:${mm}`,
      });
    },
    [settlementInput, viewYear, viewMonth],
  );

  // Calendar data
  const selected = settlementInput ? new Date(settlementInput) : null;
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

  // Dropdown data
  const thisYear = new Date().getFullYear();
  const monthItems = MONTHS.map((m, i) => ({
    value: String(i),
    label: m,
    selected: i === viewMonth,
  }));
  const yearItems = Array.from({ length: 11 }, (_, i) => {
    const y = thisYear + i;
    return { value: String(y), label: String(y), selected: y === viewYear };
  });
  const hourItems = Array.from({ length: 12 }, (_, i) => {
    const h = i + 1;
    return { value: String(h), label: String(h), selected: h === hour12 };
  });
  const minuteItems = Array.from({ length: 12 }, (_, i) => {
    const m = i * 5;
    return {
      value: String(m),
      label: String(m).padStart(2, "0"),
      selected: m === minute,
    };
  });
  const ampmItems = [
    { value: "AM", label: "AM", selected: !isPM },
    { value: "PM", label: "PM", selected: isPM },
  ];

  // Day cells
  const blanks = Array.from({ length: firstDay }, (_, i) => (
    <div key={`blank-${i}`} className="h-9 w-9" />
  ));
  const days = Array.from({ length: daysInMonth }, (_, i) => {
    const d = i + 1;
    const isSelected = d === selectedDay;
    const isToday = d === todayDay;
    let cls =
      "h-9 w-9 rounded-lg text-sm transition-colors cursor-pointer flex items-center justify-center";
    if (isSelected) {
      cls += " bg-emerald-400 text-slate-950 font-semibold";
    } else if (isToday) {
      cls +=
        " ring-1 ring-emerald-400/40 text-slate-100 hover:bg-slate-700/60";
    } else {
      cls += " text-slate-300 hover:bg-slate-700/60";
    }
    return (
      <button
        key={d}
        type="button"
        onClick={() => pickDay(d)}
        className={cls}
      >
        {d}
      </button>
    );
  });

  return (
    <div className="relative" ref={pickerRef}>
      <button
        type="button"
        onClick={togglePicker}
        className="dc-input flex items-center gap-2 cursor-pointer text-left"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
          className="shrink-0 text-slate-400"
        >
          <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
          <line x1="16" y1="2" x2="16" y2="6" />
          <line x1="8" y1="2" x2="8" y2="6" />
          <line x1="3" y1="10" x2="21" y2="10" />
        </svg>
        <span
          className={
            settlementInput ? "text-slate-100" : "text-slate-500"
          }
        >
          {settlementDisplay}
        </span>
      </button>
      {pickerOpen && (
        <div className="absolute top-full left-0 z-50 mt-1 w-[300px] rounded-xl border border-slate-700 bg-slate-900 p-4 shadow-[0_12px_32px_rgba(0,0,0,0.5)]">
          <div className="mb-3 flex items-center justify-between">
            <button
              type="button"
              onClick={prevMonth}
              className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-400 transition-colors hover:bg-slate-800 hover:text-slate-200"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <polyline points="15 18 9 12 15 6" />
              </svg>
            </button>
            <div className="flex items-center gap-1.5">
              <MiniDropdown
                name="month"
                label={MONTHS[viewMonth]}
                items={monthItems}
              />
              <MiniDropdown
                name="year"
                label={String(viewYear)}
                items={yearItems}
              />
            </div>
            <button
              type="button"
              onClick={nextMonth}
              className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-400 transition-colors hover:bg-slate-800 hover:text-slate-200"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <polyline points="9 18 15 12 9 6" />
              </svg>
            </button>
          </div>

          <div className="mb-1 grid grid-cols-7 text-center">
            {DAY_NAMES.map((d) => (
              <div
                key={d}
                className="flex h-9 w-9 items-center justify-center text-[11px] font-medium uppercase text-slate-500"
              >
                {d}
              </div>
            ))}
          </div>

          <div className="grid grid-cols-7 text-center">
            {blanks}
            {days}
          </div>

          <div className="mt-3 flex items-center gap-2 border-t border-slate-700/60 pt-3">
            <span className="text-xs text-slate-400">Time</span>
            <MiniDropdown
              name="hour"
              label={String(hour12)}
              items={hourItems}
            />
            <span className="text-slate-500">:</span>
            <MiniDropdown
              name="minute"
              label={String(minute).padStart(2, "0")}
              items={minuteItems}
            />
            <MiniDropdown
              name="ampm"
              label={isPM ? "PM" : "AM"}
              items={ampmItems}
            />
          </div>
        </div>
      )}
    </div>
  );
}
