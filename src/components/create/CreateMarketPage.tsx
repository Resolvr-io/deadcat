import { useCallback } from "react";
import { useCreateMarket } from "../../queries/mutations/useMarketCreation";
import { DEFAULT_TX_OPTIONS } from "../../services/tx";
import { useStore } from "../../store";
import { showToast } from "../shared/Toast";
import CategoryDropdown from "./CategoryDropdown";
import SettlementPicker from "./SettlementPicker";

function defaultSettlementInput(): string {
  const inThirtyDays = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000);
  const year = inThirtyDays.getFullYear();
  const month = String(inThirtyDays.getMonth() + 1).padStart(2, "0");
  const day = String(inThirtyDays.getDate()).padStart(2, "0");
  const hours = String(inThirtyDays.getHours()).padStart(2, "0");
  const minutes = String(inThirtyDays.getMinutes()).padStart(2, "0");
  return `${year}-${month}-${day}T${hours}:${minutes}`;
}

export default function CreateMarketPage() {
  const question = useStore((s) => s.createQuestion);
  const description = useStore((s) => s.createDescription);
  const category = useStore((s) => s.createCategory);
  const resolutionSource = useStore((s) => s.createResolutionSource);
  const cptSats = useStore((s) => s.createCptSats);
  const settlementInput = useStore((s) => s.createSettlementInput);
  const marketCreating = useStore((s) => s.marketCreating);

  const createMarket = useCreateMarket();

  const settlementDisplay = settlementInput
    ? new Date(settlementInput).toLocaleString("en-US", {
        weekday: "short",
        month: "short",
        day: "numeric",
        year: "numeric",
        hour: "numeric",
        minute: "2-digit",
      })
    : "Not set";

  const handleCancel = useCallback(() => {
    useStore.setState({
      createCategoryOpen: false,
      createSettlementPickerOpen: false,
      createSettlementPickerDropdown: "",
      view: "home",
    });
  }, []);

  const handleSubmit = useCallback(async () => {
    const state = useStore.getState();
    const q = state.createQuestion.trim();
    const desc = state.createDescription.trim();
    const source = state.createResolutionSource.trim();

    if (!q || !desc || !source || !state.createSettlementInput) {
      showToast(
        "Complete question, settlement rule, source, and settlement deadline before creating.",
        "error",
      );
      return;
    }

    // Check identity + wallet requirements
    if (!state.nostrPubkey || state.walletStatus !== "unlocked") {
      if (!state.nostrPubkey) {
        useStore.setState({
          setupRequires: "identity+wallet",
          onboardingStep: "nostr",
          setupModalOpen: true,
        });
      } else if (state.walletStatus === "not_created") {
        useStore.setState({
          setupRequires: "identity+wallet",
          onboardingStep: "wallet",
          onboardingNostrDone: true,
          onboardingWalletOnly: true,
          setupModalOpen: true,
          onboardingBackupScanning: true,
          onboardingBackupFound: false,
          onboardingWalletMode: "create",
        });
      } else {
        useStore.setState({
          setupRequires: "identity+wallet",
          onboardingStep: null,
          setupModalOpen: true,
        });
      }
      return;
    }

    const deadlineUnix = Math.floor(
      new Date(state.createSettlementInput).getTime() / 1000,
    );

    useStore.setState({ marketCreating: true });

    try {
      const result = await createMarket.mutateAsync({
        question: q,
        description: desc,
        category: state.createCategory,
        resolution_source: source,
        settlement_deadline_unix: deadlineUnix,
        collateral_per_token: state.createCptSats,
        tx_options: DEFAULT_TX_OPTIONS,
      });

      useStore.setState({
        view: "home",
        createQuestion: "",
        createDescription: "",
        createResolutionSource: "",
        createCptSats: 5000,
        createSettlementInput: defaultSettlementInput(),
      });

      const txid = result.market.anchor?.creation_txid ?? "unknown";
      const feeSuffix =
        result.fee.amountSat > 0
          ? ` (fee: ${result.fee.amountSat.toLocaleString()} sats)`
          : "";
      showToast(`Market created! txid: ${txid}${feeSuffix}`, "success");
    } catch (error) {
      showToast(`Failed to create market: ${error}`, "error");
    } finally {
      useStore.setState({ marketCreating: false });
    }
  }, [createMarket]);

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="mx-auto grid max-w-[1180px] gap-5 xl:grid-cols-[1.618fr_1fr]">
        {/* Left: Form */}
        <section className="rounded-xl border border-slate-800 bg-slate-950/55 p-5 lg:p-8">
          <button
            type="button"
            onClick={handleCancel}
            className="mb-3 flex items-center gap-1 text-sm text-slate-400 transition hover:text-slate-200"
          >
            <svg
              aria-hidden="true"
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
            Markets
          </button>
          <div className="mb-5">
            <p className="panel-subtitle">Prediction Contract</p>
            <h1 className="phi-title text-xl font-medium text-slate-100 lg:text-2xl">
              Create New Market
            </h1>
          </div>

          <div className="space-y-4">
            {/* Question */}
            <div>
              <label
                htmlFor="create-question"
                className="mb-1 block text-xs text-slate-400"
              >
                Question
              </label>
              <input
                id="create-question"
                value={question}
                onChange={(e) =>
                  useStore.setState({ createQuestion: e.target.value })
                }
                maxLength={140}
                className="dc-input"
                placeholder="Will X happen by Y?"
              />
            </div>

            {/* Description */}
            <div>
              <label
                htmlFor="create-description"
                className="mb-1 block text-xs text-slate-400"
              >
                Settlement rule
              </label>
              <textarea
                id="create-description"
                rows={3}
                maxLength={280}
                value={description}
                onChange={(e) =>
                  useStore.setState({ createDescription: e.target.value })
                }
                className="dc-input h-auto"
                placeholder="Define exactly how YES/NO resolves."
              />
            </div>

            {/* Category + Settlement */}
            <div className="grid gap-4 md:grid-cols-2">
              <div>
                <span className="mb-1 block text-xs text-slate-400">
                  Category
                </span>
                <CategoryDropdown />
              </div>
              <div>
                <span className="mb-1 block text-xs text-slate-400">
                  Settlement deadline
                </span>
                <SettlementPicker />
              </div>
            </div>

            {/* Resolution source */}
            <div>
              <label
                htmlFor="create-resolution-source"
                className="mb-1 block text-xs text-slate-400"
              >
                Resolution source
              </label>
              <input
                id="create-resolution-source"
                value={resolutionSource}
                onChange={(e) =>
                  useStore.setState({
                    createResolutionSource: e.target.value,
                  })
                }
                maxLength={120}
                className="dc-input"
                placeholder="Official source (e.g., NHC advisory, FEC filing, exchange index)"
              />
            </div>

            {/* Collateral per token */}
            <div>
              <label
                htmlFor="create-cpt-sats"
                className="mb-1 block text-xs text-slate-400"
              >
                Collateral per token (sats)
              </label>
              <input
                id="create-cpt-sats"
                type="number"
                min={1}
                step={1}
                value={cptSats}
                onChange={(e) =>
                  useStore.setState({
                    createCptSats: Math.max(1, Number(e.target.value) || 1),
                  })
                }
                className="dc-input"
                placeholder="5000"
              />
              <p className="mt-1 text-xs text-slate-500">
                Full contract payout = 2 &times; this value. Default: 5,000 sats
                (YES + NO = 10,000).
              </p>
            </div>
          </div>
        </section>

        {/* Right: Preview + Submit */}
        <aside className="rounded-xl border border-slate-800 bg-slate-900/80 p-5">
          <p className="panel-subtitle">Preview</p>
          <h3 className="panel-title mb-3 text-lg">New Contract Ticket</h3>
          <div className="space-y-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3">
            <p className="text-sm text-slate-200">
              {question.trim() || "Your market question will appear here."}
            </p>
            <p className="text-xs text-slate-400">
              {description.trim() ||
                "Settlement rule summary will appear here."}
            </p>
            <p className="text-xs text-slate-400">
              Category: <span className="text-slate-200">{category}</span>
            </p>
            <p className="text-xs text-slate-400">
              Settlement deadline:{" "}
              <span className="text-slate-200">
                {settlementInput ? settlementDisplay : "Not set"}
              </span>
            </p>
            <p className="text-xs text-slate-400">
              Resolution source:{" "}
              <span className="text-slate-200">
                {resolutionSource.trim() || "Not set"}
              </span>
            </p>
            <p className="text-xs text-slate-400">
              Contract value:{" "}
              <span className="text-slate-200">
                YES + NO = {(cptSats * 2).toLocaleString()} sats
              </span>
            </p>
          </div>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={marketCreating}
            className="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950 disabled:opacity-50"
          >
            {marketCreating ? "Creating Market..." : "Create Market"}
          </button>
          <p className="mt-2 text-xs text-slate-500">
            {marketCreating
              ? "Building transaction, broadcasting, and announcing. This may take a moment."
              : "Creates the on-chain contract and announces the market. Your key is the oracle signing key."}
          </p>
        </aside>
      </div>
    </div>
  );
}
