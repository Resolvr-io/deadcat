import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

type ToastKind = "success" | "error" | "warning" | "info";

interface ToastEntry {
  id: number;
  message: string;
  kind: ToastKind;
}

// ── Module-level state + subscribers ───────────────────────────────
let nextId = 0;
let toasts: ToastEntry[] = [];
const listeners = new Set<() => void>();

function notify() {
  for (const fn of listeners) fn();
}

/** Imperative toast API -- call from anywhere (no hook required). */
export function showToast(message: string, kind: ToastKind = "info"): void {
  const id = nextId++;
  toasts = [...toasts, { id, message, kind }];
  notify();
}

function dismiss(id: number) {
  toasts = toasts.filter((t) => t.id !== id);
  notify();
}

function useToasts(): ToastEntry[] {
  const [, forceRender] = useState(0);
  useEffect(() => {
    const cb = () => forceRender((n) => n + 1);
    listeners.add(cb);
    return () => {
      listeners.delete(cb);
    };
  }, []);
  return toasts;
}

// ── Single toast item ──────────────────────────────────────────────
const kindStyles: Record<ToastKind, string> = {
  success: "border-emerald-500/50 text-emerald-300",
  error: "border-red-500/50 text-red-300",
  warning: "border-amber-500/50 text-amber-300",
  info: "border-slate-600 text-slate-300",
};

function ToastItem({ entry }: { entry: ToastEntry }) {
  const [visible, setVisible] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    // fade in on mount
    requestAnimationFrame(() => setVisible(true));

    // auto-dismiss after 3 s (fade-out starts, then remove)
    timerRef.current = setTimeout(() => {
      setVisible(false);
      setTimeout(() => dismiss(entry.id), 300);
    }, 3000);

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [entry.id]);

  const handleClick = useCallback(() => {
    setVisible(false);
    setTimeout(() => dismiss(entry.id), 300);
  }, [entry.id]);

  return (
    <output
      onClick={handleClick}
      className={`max-w-lg w-[90vw] px-4 py-3 rounded-lg border bg-slate-950 ${kindStyles[entry.kind]} text-sm shadow-lg transition-opacity duration-300 cursor-pointer select-text break-all`}
      style={{ opacity: visible ? 1 : 0 }}
    >
      {entry.message}
    </output>
  );
}

// ── Portal container ───────────────────────────────────────────────
export function ToastContainer() {
  const items = useToasts();

  if (items.length === 0) return null;

  return createPortal(
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[999] flex flex-col-reverse items-center gap-2 pointer-events-auto">
      {items.map((t) => (
        <ToastItem key={t.id} entry={t} />
      ))}
    </div>,
    document.body,
  );
}
