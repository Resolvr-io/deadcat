import QRCode from "qrcode";
import { useCallback, useEffect } from "react";
import {
  useCreateBitcoinReceive,
  useCreateLightningReceive,
  useGenerateLiquidAddress,
} from "../../queries/mutations/useWalletOps";
import { useStore } from "../../store";
import { btcLabel } from "../../utils-react/wallet";
import { showToast } from "../shared/Toast";

const QR_LOGO_SVG =
  "data:image/svg+xml;base64," +
  btoa(
    '<svg width="334" height="341" viewBox="0 0 334 341" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M0.19 11.59C0.19 1.58004 13.98 -4.04996 21.51 3.46004L110.88 91.89C128.28 87.15 147.01 84.56 166.53 84.56C186.05 84.56 204.79 87.14 222.19 91.89L311.54 3.47004C319.06 -4.02996 332.86 1.59004 332.86 11.6V206.56C332.98 208.58 333.05 210.61 333.05 212.65C333.05 283.39 258.5 340.74 166.53 340.74C74.56 340.74 0 283.4 0 212.65C0 210.61 0.06 208.58 0.19 206.56V11.59Z" fill="black"/><path d="M128.46 239.55L154.85 265.26V267.59C154.85 279.12 146.28 288.5 135.74 288.51H116.57C111.62 288.51 107.6 292.47 107.6 297.33C107.6 302.19 111.63 306.16 116.57 306.16H135.74C146.7 306.16 157 301.08 163.98 292.54C170.95 301.07 181.25 306.16 192.22 306.16C212.66 306.16 229.28 288.86 229.28 267.59C229.28 262.72 225.25 258.76 220.3 258.76C215.35 258.76 211.32 262.72 211.32 267.59C211.32 279.12 202.75 288.51 192.21 288.51C181.67 288.51 173.1 279.13 173.1 267.59V265.21L199.44 239.55H128.44H128.46ZM90.2699 179.49L67.1499 156.37L56.3599 167.16L79.4799 190.28L56.4399 213.32L67.2299 224.11L90.2699 201.07L113.39 224.19L124.18 213.4L101.06 190.28L124.26 167.09L113.47 156.3L90.2699 179.5V179.49ZM250.25 158.27C256.89 164.96 261.31 176.78 261.31 190.24C261.31 202.78 257.48 213.89 251.59 220.76C277 217.42 295.9 204.74 295.9 189.6C295.9 174.46 276.33 161.34 250.26 158.27H250.25ZM224.79 158.45C199.45 161.82 180.61 174.48 180.61 189.59C180.61 204.7 198.79 216.92 223.46 220.55C217.66 213.66 213.91 202.65 213.91 190.23C213.91 176.9 218.24 165.17 224.78 158.45H224.79Z" fill="#34D399"/></svg>',
  );

async function generateQr(value: string): Promise<string> {
  try {
    const canvas = document.createElement("canvas");
    await QRCode.toCanvas(canvas, value, {
      errorCorrectionLevel: "H",
      margin: 4,
      scale: 8,
      color: { dark: "#0f172a", light: "#ffffff" },
    });
    const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
    const logoImg = new Image();
    logoImg.src = QR_LOGO_SVG;
    await new Promise<void>((resolve, reject) => {
      logoImg.onload = () => resolve();
      logoImg.onerror = () => reject();
    });
    const logoSize = Math.floor(canvas.width * 0.22);
    const x = Math.floor((canvas.width - logoSize) / 2);
    const y = Math.floor((canvas.height - logoSize) / 2);
    const pad = 10;
    ctx.fillStyle = "#ffffff";
    ctx.beginPath();
    ctx.roundRect(x - pad, y - pad, logoSize + pad * 2, logoSize + pad * 2, 6);
    ctx.fill();
    ctx.drawImage(logoImg, x, y, logoSize, logoSize);
    return canvas.toDataURL("image/png");
  } catch {
    return "";
  }
}

function Copyable({ value, label }: { value: string; label: string }) {
  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(value);
    showToast("Copied to clipboard");
  }, [value]);

  return (
    <div className="flex items-center gap-2">
      <div className="flex-1 overflow-hidden rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
        <div className="text-xs text-slate-500">{label}</div>
        <div className="mono text-xs text-slate-300 truncate">{value}</div>
      </div>
      <button
        type="button"
        onClick={handleCopy}
        className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800"
      >
        Copy
      </button>
    </div>
  );
}

export function ReceiveModal() {
  const walletModalTab = useStore((s) => s.walletModalTab);
  const receiveAmount = useStore((s) => s.receiveAmount);
  const receiveCreating = useStore((s) => s.receiveCreating);
  const receiveError = useStore((s) => s.receiveError);
  const receiveLightningSwap = useStore((s) => s.receiveLightningSwap);
  const receiveLiquidAddress = useStore((s) => s.receiveLiquidAddress);
  const receiveLiquidLoading = useStore((s) => s.receiveLiquidLoading);
  const receiveBitcoinSwap = useStore((s) => s.receiveBitcoinSwap);
  const receiveBtcPairInfo = useStore((s) => s.receiveBtcPairInfo);
  const modalQr = useStore((s) => s.modalQr);
  const showLbtcLabel = useStore((s) => s.showLbtcLabel);

  const createLightning = useCreateLightningReceive();
  const createBitcoin = useCreateBitcoinReceive();
  const generateAddress = useGenerateLiquidAddress();

  // Auto-generate liquid address when tab becomes liquid
  useEffect(() => {
    if (walletModalTab === "liquid" && !receiveLiquidAddress) {
      useStore.setState({ receiveLiquidLoading: true });
      generateAddress.mutate(undefined, {
        onSuccess: async (address) => {
          const qr = await generateQr(address);
          useStore.setState({
            receiveLiquidAddress: address,
            receiveLiquidLoading: false,
            modalQr: qr,
          });
        },
        onError: () => {
          useStore.setState({ receiveLiquidLoading: false });
        },
      });
    }
  }, [walletModalTab, receiveLiquidAddress, generateAddress.mutate]); // eslint-disable-line react-hooks/exhaustive-deps

  const handlePreset = useCallback((preset: string) => {
    useStore.setState({ receiveAmount: preset });
  }, []);

  const handleCreateLightning = useCallback(() => {
    const amt = Math.floor(Number(receiveAmount) || 0);
    if (amt <= 0) {
      useStore.setState({ receiveError: "Enter a valid amount." });
      return;
    }
    useStore.setState({ receiveCreating: true, receiveError: "" });
    createLightning.mutate(
      { amountSat: amt },
      {
        onSuccess: async (swap) => {
          const qr = await generateQr(swap.invoice);
          useStore.setState({
            receiveLightningSwap: swap,
            receiveCreating: false,
            modalQr: qr,
          });
        },
        onError: (e) => {
          useStore.setState({
            receiveError: String(e),
            receiveCreating: false,
          });
        },
      },
    );
  }, [receiveAmount, createLightning]);

  const handleCreateBitcoin = useCallback(() => {
    const amt = Math.floor(Number(receiveAmount) || 0);
    if (amt <= 0) {
      useStore.setState({ receiveError: "Enter a valid amount." });
      return;
    }
    useStore.setState({ receiveCreating: true, receiveError: "" });
    createBitcoin.mutate(
      { amountSat: amt },
      {
        onSuccess: async (swap) => {
          const qr = await generateQr(swap.bip21 || swap.lockupAddress);
          useStore.setState({
            receiveBitcoinSwap: swap,
            receiveCreating: false,
            modalQr: qr,
          });
        },
        onError: (e) => {
          useStore.setState({
            receiveError: String(e),
            receiveCreating: false,
          });
        },
      },
    );
  }, [receiveAmount, createBitcoin]);

  const creating =
    receiveCreating || createLightning.isPending || createBitcoin.isPending;

  let content: React.ReactNode = null;

  if (walletModalTab === "lightning") {
    if (receiveLightningSwap) {
      const s = receiveLightningSwap;
      content = (
        <div className="space-y-3">
          <p className="text-sm font-semibold text-slate-100">Invoice Ready</p>
          <p className="text-xs text-slate-400">
            Swap {s.id.slice(0, 8)}... |{" "}
            {s.expectedOnchainAmountSat.toLocaleString()} sats expected on
            Liquid
          </p>
          <p className="text-xs text-slate-500">
            Expires: {new Date(s.invoiceExpiresAt).toLocaleString()}
          </p>
          {modalQr && (
            <div className="flex justify-center">
              <img src={modalQr} alt="QR" className="w-56 h-56 rounded-lg" />
            </div>
          )}
          <Copyable value={s.invoice} label="BOLT11 Invoice" />
        </div>
      );
    } else {
      content = (
        <div className="space-y-3">
          <p className="text-sm text-slate-400">
            Create a Lightning invoice via Boltz. Funds settle as{" "}
            {btcLabel(showLbtcLabel)}.
          </p>
          <div className="flex gap-2">
            {["1000", "10000", "100000"].map((preset) => (
              <button
                type="button"
                key={preset}
                onClick={() => handlePreset(preset)}
                className="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800"
              >
                {preset === "1000" ? "1k" : preset === "10000" ? "10k" : "100k"}
              </button>
            ))}
          </div>
          <div className="flex gap-2">
            <input
              id="receive-amount"
              type="number"
              min="1"
              value={receiveAmount}
              onChange={(e) =>
                useStore.setState({ receiveAmount: e.target.value })
              }
              placeholder="Amount (sats)"
              className="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2"
            />
            <button
              type="button"
              onClick={handleCreateLightning}
              className="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"
              disabled={creating}
            >
              {creating ? "Creating..." : "Create Invoice"}
            </button>
          </div>
        </div>
      );
    }
  } else if (walletModalTab === "liquid") {
    if (receiveLiquidAddress) {
      content = (
        <div className="space-y-3">
          <p className="text-sm text-slate-400">
            Send L-BTC to this address to fund your wallet.
          </p>
          {modalQr && (
            <div className="flex justify-center">
              <img src={modalQr} alt="QR" className="w-56 h-56 rounded-lg" />
            </div>
          )}
          <Copyable value={receiveLiquidAddress} label="Liquid Address" />
        </div>
      );
    } else {
      content = (
        <div className="flex flex-col items-center gap-4 py-4">
          <p className="text-sm text-slate-400">
            {receiveLiquidLoading
              ? "Generating address..."
              : "Loading address..."}
          </p>
        </div>
      );
    }
  } else {
    // Bitcoin tab
    if (receiveBitcoinSwap) {
      const s = receiveBitcoinSwap;
      content = (
        <div className="space-y-3">
          <p className="text-sm font-semibold text-slate-100">
            Bitcoin Deposit Address Ready
          </p>
          <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3">
            <p className="text-xs font-medium uppercase tracking-wider text-amber-200">
              Send Exactly
            </p>
            <p className="mt-1 text-lg font-semibold text-amber-100">
              {s.amountSat.toLocaleString()} sats
            </p>
            <p className="mt-1 text-xs text-amber-100/70">
              Send this exact Bitcoin amount to the lockup address below to
              complete the swap.
            </p>
          </div>
          <p className="text-xs text-slate-400">
            Swap {s.id.slice(0, 8)}... | {s.expectedAmountSat.toLocaleString()}{" "}
            sats expected on Liquid
          </p>
          <p className="text-xs text-slate-500">
            Timeout block: {s.timeoutBlockHeight}
          </p>
          {modalQr && (
            <div className="flex justify-center">
              <img src={modalQr} alt="QR" className="w-56 h-56 rounded-lg" />
            </div>
          )}
          <Copyable value={s.lockupAddress} label="Bitcoin Lockup Address" />
          {s.bip21 && <Copyable value={s.bip21} label="BIP21 URI" />}
        </div>
      );
    } else {
      content = (
        <div className="space-y-3">
          <p className="text-sm text-slate-400">
            Send BTC on-chain to receive {btcLabel(showLbtcLabel)} via Boltz.
          </p>
          {receiveBtcPairInfo && (
            <div className="rounded-lg border border-slate-700 bg-slate-900 p-3 text-xs text-slate-400 space-y-1">
              <div>
                Min: {receiveBtcPairInfo.minAmountSat.toLocaleString()} sats
              </div>
              <div>
                Max: {receiveBtcPairInfo.maxAmountSat.toLocaleString()} sats
              </div>
              <div>
                Fee: {receiveBtcPairInfo.feePercentage}% +{" "}
                {receiveBtcPairInfo.fixedMinerFeeTotalSat.toLocaleString()} sats
                fixed
              </div>
            </div>
          )}
          <div className="flex gap-2">
            <input
              id="receive-amount"
              type="number"
              min="1"
              value={receiveAmount}
              onChange={(e) =>
                useStore.setState({ receiveAmount: e.target.value })
              }
              placeholder="Amount (sats)"
              className="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2"
            />
            <button
              type="button"
              onClick={handleCreateBitcoin}
              className="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"
              disabled={creating}
            >
              {creating ? "Creating..." : "Create Address"}
            </button>
          </div>
        </div>
      );
    }
  }

  return (
    <>
      {content}
      {receiveError && (
        <div className="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">
          {receiveError}
        </div>
      )}
    </>
  );
}
