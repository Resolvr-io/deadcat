/**
 * Pulsing placeholder that mirrors the CommentRow layout. Rendered
 * inside `<ul className="divide-y …">` so the dividers + spacing
 * match live rows and the section doesn't reflow when real data
 * lands.
 */
export function CommentRowSkeleton({
  /** Pseudo-random length variation so stacked skeletons don't look
   *  like a single pattern tiled. 0 = short, 1 = medium, 2 = long. */
  variant = 1,
}: {
  variant?: 0 | 1 | 2;
}) {
  const bodyLine1 = ["w-1/2", "w-3/4", "w-[90%]"][variant];
  const bodyLine2Visible = variant >= 1;
  const bodyLine2 = ["", "w-1/3", "w-2/3"][variant];

  return (
    <div className="flex animate-pulse items-start gap-3 py-3">
      <div className="h-8 w-8 shrink-0 rounded-full bg-slate-800" />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <div className="h-3.5 w-24 rounded bg-slate-800" />
          <div className="h-2.5 w-12 rounded bg-slate-800/70" />
        </div>
        <div className="mt-2 space-y-1.5">
          <div className={`h-3 rounded bg-slate-800 ${bodyLine1}`} />
          {bodyLine2Visible && (
            <div className={`h-3 rounded bg-slate-800 ${bodyLine2}`} />
          )}
        </div>
        <div className="mt-3 flex items-center gap-3">
          <div className="h-4 w-4 rounded-full bg-slate-800" />
          <div className="h-4 w-4 rounded-full bg-slate-800" />
          <div className="h-4 w-6 rounded-full bg-slate-800" />
        </div>
      </div>
    </div>
  );
}
