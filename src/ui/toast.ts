export function showToast(
  message: string,
  kind: "success" | "error" | "info" = "info",
) {
  const el = document.createElement("div");
  const style =
    kind === "success"
      ? "border-emerald-500/50 text-emerald-300"
      : kind === "error"
        ? "border-red-500/50 text-red-300"
        : "border-slate-600 text-slate-300";
  el.className = `fixed bottom-6 left-1/2 -translate-x-1/2 z-[999] max-w-lg w-[90vw] px-4 py-3 rounded-lg border bg-slate-950 ${style} text-sm shadow-lg transition-opacity duration-300`;
  el.style.opacity = "0";
  el.style.userSelect = "text";
  el.style.wordBreak = "break-all";
  el.textContent = message;
  document.body.appendChild(el);
  requestAnimationFrame(() => (el.style.opacity = "1"));
  setTimeout(() => {
    el.style.opacity = "0";
    setTimeout(() => el.remove(), 300);
  }, 6000);
}
