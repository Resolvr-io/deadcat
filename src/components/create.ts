import { categories, SATS_PER_FULL_CONTRACT, state } from "../state.ts";

export function renderCreateMarket(): string {
  const yesSats = Math.max(
    1,
    Math.min(
      SATS_PER_FULL_CONTRACT - 1,
      Math.round(state.createStartingYesSats),
    ),
  );
  const noSats = SATS_PER_FULL_CONTRACT - yesSats;
  const settlementLabel = state.createSettlementInput
    ? new Date(state.createSettlementInput).toLocaleString("en-US", {
        weekday: "short",
        month: "short",
        day: "numeric",
        year: "numeric",
        hour: "numeric",
        minute: "2-digit",
      })
    : "Not set";

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="mx-auto grid max-w-[1180px] gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[34px]">
          <div class="mb-5 flex items-center justify-between gap-3">
            <div>
              <p class="panel-subtitle">Prediction Contract</p>
              <h1 class="phi-title text-xl font-medium text-slate-100 lg:text-2xl">Create New Market</h1>
            </div>
            <button data-action="cancel-create-market" class="rounded-lg border border-slate-700 px-3 py-2 text-sm text-slate-300">Back</button>
          </div>

          <div class="space-y-4">
            <div>
              <label for="create-question" class="mb-1 block text-xs text-slate-400">Question</label>
              <input id="create-question" value="${state.createQuestion}" maxlength="140" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Will X happen by Y?" />
            </div>

            <div>
              <label for="create-description" class="mb-1 block text-xs text-slate-400">Settlement rule</label>
              <textarea id="create-description" rows="3" maxlength="280" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Define exactly how YES/NO resolves.">${state.createDescription}</textarea>
            </div>

            <div class="grid gap-4 md:grid-cols-2">
              <div>
                <label for="create-category" class="mb-1 block text-xs text-slate-400">Category</label>
                <select id="create-category" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm">
                  ${categories
                    .filter((item) => item !== "Trending")
                    .map(
                      (item) =>
                        `<option value="${item}" ${state.createCategory === item ? "selected" : ""}>${item}</option>`,
                    )
                    .join("")}
                </select>
              </div>

              <div>
                <label for="create-settlement" class="mb-1 block text-xs text-slate-400">Settlement deadline</label>
                <input id="create-settlement" type="datetime-local" value="${state.createSettlementInput}" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
              </div>
            </div>

            <div>
              <label for="create-resolution-source" class="mb-1 block text-xs text-slate-400">Resolution source</label>
              <input id="create-resolution-source" value="${state.createResolutionSource}" maxlength="120" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Official source (e.g., NHC advisory, FEC filing, exchange index)" />
            </div>

            <div>
              <label for="create-yes-sats" class="mb-1 block text-xs text-slate-400">Starting Yes price (sats out of 100)</label>
              <input id="create-yes-sats" type="number" min="1" max="99" step="1" value="${yesSats}" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
            </div>
          </div>
        </section>

        <aside class="rounded-[21px] border border-slate-800 bg-slate-900/80 p-[21px]">
          <p class="panel-subtitle">Preview</p>
          <h3 class="panel-title mb-3 text-lg">New Contract Ticket</h3>
          <div class="space-y-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3">
            <p class="text-sm text-slate-200">${state.createQuestion.trim() || "Your market question will appear here."}</p>
            <p class="text-xs text-slate-400">${state.createDescription.trim() || "Settlement rule summary will appear here."}</p>
            <div class="grid grid-cols-2 gap-2">
              <div class="rounded-lg border border-slate-800 bg-slate-900/60 p-2 text-center text-emerald-400">Yes ${yesSats} sats</div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/60 p-2 text-center text-rose-400">No ${noSats} sats</div>
            </div>
            <p class="text-xs text-slate-400">Category: <span class="text-slate-200">${state.createCategory}</span></p>
            <p class="text-xs text-slate-400">Settlement deadline: <span class="text-slate-200">${settlementLabel}</span></p>
            <p class="text-xs text-slate-400">Resolution source: <span class="text-slate-200">${state.createResolutionSource.trim() || "Not set"}</span></p>
            <p class="text-xs text-slate-500">Yes + No = ${SATS_PER_FULL_CONTRACT} sats</p>
          </div>
          <button data-action="submit-create-market" class="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950 disabled:opacity-50" ${state.marketCreating ? "disabled" : ""}>${state.marketCreating ? "Creating Market..." : "Create Market"}</button>
          <p class="mt-2 text-xs text-slate-500">${state.marketCreating ? "Building transaction, broadcasting, and announcing. This may take a moment." : "Creates the on-chain contract and announces the market. Your key is the oracle signing key."}</p>
        </aside>
      </div>
    </div>
  `;
}
