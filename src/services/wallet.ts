import { invoke } from "@tauri-apps/api/core";
import QRCode from "qrcode";
import { state, markets } from "../state";
import type { WalletNetwork, ChainTipResponse, PaymentSwap } from "../types";
import { showOverlayLoader, hideOverlayLoader } from "../ui/loader";

export function formatLbtc(sats: number): string {
  if (state.walletUnit === "sats") {
    return sats.toLocaleString() + " L-sats";
  }
  const btc = sats / 100_000_000;
  return btc.toFixed(8) + " L-BTC";
}

export async function fetchWalletStatus(): Promise<void> {
  try {
    const appState = await invoke<{
      walletStatus: "not_created" | "locked" | "unlocked";
      networkStatus: { network: string; policyAssetId: string };
    }>("get_app_state");
    state.walletStatus = appState.walletStatus;
    state.walletNetwork = appState.networkStatus.network as
      | "mainnet"
      | "testnet"
      | "regtest";
    state.walletPolicyAssetId = appState.networkStatus.policyAssetId;
  } catch (e) {
    console.warn("Failed to fetch app state:", e);
  }
}

export async function refreshWallet(render: () => void): Promise<void> {
  state.walletLoading = true;
  state.walletError = "";
  showOverlayLoader("Syncing wallet...");
  render();
  try {
    await invoke("sync_wallet");
    const [balance, txs, swaps] = await Promise.all([
      invoke<{ assets: Record<string, number> }>("get_wallet_balance"),
      invoke<
        {
          txid: string;
          balanceChange: number;
          fee: number;
          height: number | null;
          timestamp: number | null;
          txType: string;
        }[]
      >("get_wallet_transactions"),
      invoke<PaymentSwap[]>("list_payment_swaps"),
    ]);
    state.walletBalance = balance.assets;
    state.walletTransactions = txs;
    state.walletSwaps = swaps;
  } catch (e) {
    state.walletError = String(e);
  }
  state.walletLoading = false;
  hideOverlayLoader();
  render();
}

const QR_LOGO_SVG =
  "data:image/svg+xml;base64," +
  btoa(
    '<svg width="334" height="341" viewBox="0 0 334 341" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M0.19 11.59C0.19 1.58004 13.98 -4.04996 21.51 3.46004L110.88 91.89C128.28 87.15 147.01 84.56 166.53 84.56C186.05 84.56 204.79 87.14 222.19 91.89L311.54 3.47004C319.06 -4.02996 332.86 1.59004 332.86 11.6V206.56C332.98 208.58 333.05 210.61 333.05 212.65C333.05 283.39 258.5 340.74 166.53 340.74C74.56 340.74 0 283.4 0 212.65C0 210.61 0.06 208.58 0.19 206.56V11.59Z" fill="black"/><path d="M128.46 239.55L154.85 265.26V267.59C154.85 279.12 146.28 288.5 135.74 288.51H116.57C111.62 288.51 107.6 292.47 107.6 297.33C107.6 302.19 111.63 306.16 116.57 306.16H135.74C146.7 306.16 157 301.08 163.98 292.54C170.95 301.07 181.25 306.16 192.22 306.16C212.66 306.16 229.28 288.86 229.28 267.59C229.28 262.72 225.25 258.76 220.3 258.76C215.35 258.76 211.32 262.72 211.32 267.59C211.32 279.12 202.75 288.51 192.21 288.51C181.67 288.51 173.1 279.13 173.1 267.59V265.21L199.44 239.55H128.44H128.46ZM90.2699 179.49L67.1499 156.37L56.3599 167.16L79.4799 190.28L56.4399 213.32L67.2299 224.11L90.2699 201.07L113.39 224.19L124.18 213.4L101.06 190.28L124.26 167.09L113.47 156.3L90.2699 179.5V179.49ZM250.25 158.27C256.89 164.96 261.31 176.78 261.31 190.24C261.31 202.78 257.48 213.89 251.59 220.76C277 217.42 295.9 204.74 295.9 189.6C295.9 174.46 276.33 161.34 250.26 158.27H250.25ZM224.79 158.45C199.45 161.82 180.61 174.48 180.61 189.59C180.61 204.7 198.79 216.92 223.46 220.55C217.66 213.66 213.91 202.65 213.91 190.23C213.91 176.9 218.24 165.17 224.78 158.45H224.79Z" fill="#34D399"/></svg>',
  );

export async function generateQr(value: string): Promise<void> {
  try {
    const canvas = document.createElement("canvas");
    await QRCode.toCanvas(canvas, value, {
      errorCorrectionLevel: "H",
      margin: 4,
      scale: 8,
      color: { dark: "#0f172a", light: "#ffffff" },
    });
    const ctx = canvas.getContext("2d")!;
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
    state.modalQr = canvas.toDataURL("image/png");
  } catch {
    state.modalQr = "";
  }
}

export function resetReceiveState(): void {
  state.receiveAmount = "";
  state.receiveCreating = false;
  state.receiveError = "";
  state.receiveLightningSwap = null;
  state.receiveLiquidAddress = "";
  state.receiveBitcoinSwap = null;
  state.modalQr = "";
}

export function resetSendState(): void {
  state.sendInvoice = "";
  state.sendLiquidAddress = "";
  state.sendLiquidAmount = "";
  state.sendBtcAmount = "";
  state.sendCreating = false;
  state.sendError = "";
  state.sentLightningSwap = null;
  state.sentLiquidResult = null;
  state.sentBitcoinSwap = null;
  state.modalQr = "";
}

export function flowLabel(flow: string): string {
  switch (flow) {
    case "liquid_to_lightning":
      return "Lightning Send";
    case "lightning_to_liquid":
      return "Lightning Receive";
    case "bitcoin_to_liquid":
      return "Bitcoin Receive";
    case "liquid_to_bitcoin":
      return "Bitcoin Send";
    default:
      return flow;
  }
}

export function formatSwapStatus(status: string): string {
  return status.replace(/[._]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

export async function syncCurrentHeightFromLwk(
  network: WalletNetwork,
  render: () => void,
  updateEstClockLabels: () => void,
): Promise<void> {
  try {
    const tip = await invoke<ChainTipResponse>("fetch_chain_tip", { network });
    if (!Number.isFinite(tip.height) || tip.height <= 0) return;

    let didChange = false;
    for (const market of markets) {
      if (market.currentHeight !== tip.height) {
        market.currentHeight = tip.height;
        didChange = true;
      }
    }

    if (didChange) {
      render();
      updateEstClockLabels();
    }
  } catch (error) {
    console.warn("Failed to sync chain tip from LWK:", error);
  }
}
