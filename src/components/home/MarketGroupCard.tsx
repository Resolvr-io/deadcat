import { useStore } from "../../store";
import type { MarketGroup } from "../../types";
import { formatTimeRemaining, formatVolumeBtc } from "../../utils-react/format";

function openGroup(group: MarketGroup) {
  useStore.setState({
    selectedGroupId: group.id,
    selectedOutcomeId: null,
    view: "group",
  });
}

export default function MarketGroupCard({
  group,
  index,
}: {
  group: MarketGroup;
  index: number;
}) {
  const blocksLeft = group.expiryHeight - group.currentHeight;
  const timeLeft = blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired";

  // Top 3 outcomes by probability
  const topOutcomes = [...group.outcomes]
    .sort((a, b) => b.yesPrice - a.yesPrice)
    .slice(0, 3);

  return (
    <button
      type="button"
      onClick={() => openGroup(group)}
      className="market-card w-full cursor-pointer rounded-xl bg-slate-950/50 p-4 text-left transition hover:bg-slate-900/60"
      style={{ animationDelay: `${index * 50}ms` }}
    >
      {/* Header */}
      <div className="mb-2 flex items-center justify-between text-xs">
        <span className="text-slate-500">{group.category}</span>
        <span className="text-slate-500">{timeLeft}</span>
      </div>

      {/* Title */}
      <p className="mb-3 text-sm font-normal text-slate-200">{group.title}</p>

      {/* Top outcomes */}
      <div className="mb-3 space-y-1.5">
        {topOutcomes.map((outcome) => {
          const pct = Math.round(outcome.yesPrice * 100);
          const barWidth = Math.max(4, pct);
          return (
            <div key={outcome.id} className="flex items-center gap-2">
              <div className="relative h-5 flex-1 overflow-hidden rounded-sm bg-slate-900">
                <div
                  className="absolute inset-y-0 left-0 rounded-sm bg-emerald-500/20"
                  style={{ width: `${barWidth}%` }}
                />
                <span className="absolute inset-y-0 left-2 flex items-center text-[11px] text-slate-300">
                  {outcome.name}
                </span>
              </div>
              <span className="w-10 shrink-0 text-right text-xs font-medium text-emerald-400">
                {pct}%
              </span>
            </div>
          );
        })}
        {group.outcomes.length > 3 && (
          <p className="pl-1 text-[11px] text-slate-500">
            +{group.outcomes.length - 3} more outcomes
          </p>
        )}
      </div>

      {/* Footer */}
      <p className="text-xs text-slate-500">
        {formatVolumeBtc(group.totalVolumeBtc)} vol
        {group.traderCount > 0 && (
          <> · {group.traderCount.toLocaleString()} traders</>
        )}
        {" · "}
        {timeLeft}
      </p>
    </button>
  );
}
