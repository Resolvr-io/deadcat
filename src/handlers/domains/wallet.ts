import { openUrl } from "@tauri-apps/plugin-opener";
import { tauriApi } from "../../api/tauri.ts";
import {
  fetchWalletSnapshot,
  fetchWalletStatus,
  generateQr,
  refreshWallet,
  resetReceiveState,
  resetSendState,
  resetWalletSessionState,
  resetWalletStoredState,
  restoreWalletAndSync,
} from "../../services/wallet.ts";
import { createWalletData, markets, state } from "../../state.ts";
import {
  hideOverlayLoader,
  showOverlayLoader,
  updateOverlayMessage,
} from "../../ui/loader.ts";
import { showToast } from "../../ui/toast.ts";
import { runAsyncAction } from "./async-action.ts";
import type { ClickDomainContext } from "./context.ts";

export async function handleWalletDomain(
  ctx: ClickDomainContext,
): Promise<void> {
  const { action, actionDomain, actionEl, render, target } = ctx;
  if (actionDomain === "wallet" || actionDomain === null) {
    if (action === "create-wallet") {
      if (!state.walletPassword) {
        state.walletError = "Password is required.";
        render();
        return;
      }
      if (state.walletPassword !== state.walletPasswordConfirm) {
        state.walletError = "Passwords do not match.";
        render();
        return;
      }
      state.walletLoading = true;
      state.walletError = "";
      showOverlayLoader("Creating wallet...");
      render();
      runAsyncAction(async () => {
        try {
          const mnemonic = await tauriApi.createWallet(state.walletPassword);
          state.walletMnemonic = mnemonic;
          state.walletPassword = "";
          state.walletPasswordConfirm = "";
          await fetchWalletStatus();
          // Stay on not_created so mnemonic screen shows
          state.walletStatus = "not_created";
        } catch (e) {
          state.walletError = String(e);
        }
        state.walletLoading = false;
        hideOverlayLoader();
        render();
      });
      return;
    }

    if (action === "dismiss-mnemonic") {
      state.walletMnemonic = "";
      state.walletStatus = "locked";
      state.walletPassword = "";
      state.walletPasswordConfirm = "";
      render();
      return;
    }

    if (action === "toggle-restore") {
      state.walletShowRestore = !state.walletShowRestore;
      state.walletError = "";
      state.walletPasswordConfirm = "";
      render();
      return;
    }

    if (action === "restore-wallet") {
      if (!state.walletRestoreMnemonic.trim() || !state.walletPassword) {
        state.walletError = "Recovery phrase and password are required.";
        render();
        return;
      }
      if (state.walletPassword !== state.walletPasswordConfirm) {
        state.walletError = "Passwords do not match.";
        render();
        return;
      }
      state.walletLoading = true;
      state.walletError = "";
      showOverlayLoader("Restoring wallet...");
      render();
      runAsyncAction(async () => {
        try {
          const pw = state.walletPassword;
          await restoreWalletAndSync({
            mnemonic: state.walletRestoreMnemonic,
            password: pw,
            unlock: true,
            setStep: updateOverlayMessage,
          });
          state.walletRestoreMnemonic = "";
          state.walletPassword = "";
          state.walletPasswordConfirm = "";
          await fetchWalletStatus();
          if (state.walletStatus === "unlocked") {
            const { balance, transactions } = await fetchWalletSnapshot({
              includeSwaps: false,
            });
            state.walletData = {
              ...createWalletData(),
              balance,
              transactions,
            };
          }
          state.walletLoading = false;
          hideOverlayLoader();
          render();
          showToast("Wallet restored successfully", "success");
        } catch (e) {
          state.walletError = String(e);
          state.walletLoading = false;
          hideOverlayLoader();
          render();
        }
      });
      return;
    }

    if (action === "unlock-wallet") {
      if (!state.walletPassword) {
        state.walletError = "Password is required.";
        render();
        return;
      }
      state.walletLoading = true;
      state.walletError = "";
      showOverlayLoader("Unlocking wallet...");
      render();
      runAsyncAction(async () => {
        try {
          await tauriApi.unlockWallet(state.walletPassword);
          state.walletPassword = "";
          await fetchWalletStatus();
          // Load cached wallet data instantly (no Electrum sync)
          const { balance, transactions, swaps } = await fetchWalletSnapshot();
          state.walletData = {
            ...createWalletData(),
            balance,
            transactions,
            swaps,
          };
          state.walletLoading = false;
          hideOverlayLoader();
          render();
          // Background Electrum sync -- updates balances when done
          tauriApi
            .syncWallet()
            .then(async () => {
              const { balance: freshBalance, transactions: freshTransactions } =
                await fetchWalletSnapshot({
                  includeSwaps: false,
                });
              if (state.walletData) {
                state.walletData.balance = freshBalance;
                state.walletData.transactions = freshTransactions;
              }
              render();
            })
            .catch(() => {
              /* silent background sync failure */
            });
        } catch (e) {
          state.walletError = String(e);
          state.walletLoading = false;
          hideOverlayLoader();
          render();
        }
      });
      return;
    }

    if (action === "lock-wallet") {
      runAsyncAction(async () => {
        try {
          await tauriApi.lockWallet();
          await fetchWalletStatus();
          resetWalletSessionState();
          render();
        } catch (e) {
          state.walletError = String(e);
          render();
        }
      });
      return;
    }

    if (action === "wallet-delete-start") {
      state.walletDeletePrompt = true;
      state.walletDeleteConfirm = "";
      render();
      document
        .getElementById("wallet-delete-confirm")
        ?.scrollIntoView({ block: "center" });
      return;
    }

    if (action === "wallet-delete-cancel") {
      state.walletDeletePrompt = false;
      state.walletDeleteConfirm = "";
      render();
      return;
    }

    if (action === "wallet-delete-confirm") {
      if (state.walletDeleteConfirm.trim().toUpperCase() !== "DELETE") return;
      runAsyncAction(async () => {
        try {
          await tauriApi.deleteWallet();
          await fetchWalletStatus();
          resetWalletStoredState();
          state.walletDeletePrompt = false;
          state.walletDeleteConfirm = "";
          showToast("Wallet removed", "success");
        } catch (e) {
          showToast(`Failed to remove wallet: ${String(e)}`, "error");
        }
        render();
      });
      return;
    }

    if (action === "forgot-password-delete") {
      runAsyncAction(async () => {
        try {
          await tauriApi.deleteWallet();
          await fetchWalletStatus();
          resetWalletStoredState();
          showToast(
            "Wallet removed — restore from backup or recovery phrase",
            "info",
          );
        } catch (e) {
          showToast(`Failed to remove wallet: ${String(e)}`, "error");
        }
        render();
      });
      return;
    }

    if (action === "toggle-balance-hidden") {
      state.walletBalanceHidden = !state.walletBalanceHidden;
      render();
      return;
    }

    if (action === "toggle-utxos-expanded") {
      state.walletUtxosExpanded = !state.walletUtxosExpanded;
      render();
      return;
    }

    if (action === "toggle-mini-wallet") {
      state.showMiniWallet = !state.showMiniWallet;
      render();
      return;
    }

    if (action === "set-wallet-unit") {
      const unit = actionEl?.getAttribute("data-unit") as "sats" | "btc" | null;
      if (unit) {
        state.walletUnit = unit;
        render();
      }
      return;
    }

    if (action === "sync-wallet") {
      void refreshWallet(render);
      return;
    }

    if (action === "open-explorer-tx") {
      const txid = actionEl?.getAttribute("data-txid");
      if (txid) {
        const base =
          state.walletNetwork === "testnet"
            ? "https://blockstream.info/liquidtestnet"
            : "https://blockstream.info/liquid";
        void openUrl(`${base}/tx/${txid}`);
      }
      return;
    }

    if (action === "open-nostr-event") {
      const marketId = actionEl?.getAttribute("data-market-id");
      const nevent = actionEl?.getAttribute("data-nevent");
      const market = marketId ? markets.find((m) => m.id === marketId) : null;
      if (market?.nostrEventJson) {
        state.nostrEventModal = true;
        state.nostrEventJson = market.nostrEventJson;
        state.nostrEventNevent = nevent ?? null;
        render();
      } else {
        showToast("Nostr event data not available", "error");
      }
      return;
    }

    if (action === "nostr-event-backdrop" && actionEl === target) {
      state.nostrEventModal = false;
      state.nostrEventJson = null;
      state.nostrEventNevent = null;
      render();
      return;
    }

    if (action === "close-nostr-event-modal") {
      state.nostrEventModal = false;
      state.nostrEventJson = null;
      state.nostrEventNevent = null;
      render();
      return;
    }

    if (action === "copy-nostr-event-json") {
      if (state.nostrEventJson) {
        void navigator.clipboard.writeText(state.nostrEventJson);
        showToast("Copied to clipboard");
      }
      return;
    }

    if (action === "open-receive") {
      state.walletModal = "receive";
      state.walletModalTab = "lightning";
      resetReceiveState();
      render();
      runAsyncAction(async () => {
        try {
          const pairs = await tauriApi.getChainSwapPairs();
          state.receiveBtcPairInfo = pairs.bitcoinToLiquid;
        } catch {
          /* ignore */
        }
        render();
      });
      return;
    }

    if (action === "open-send") {
      state.walletModal = "send";
      state.walletModalTab = "lightning";
      resetSendState();
      render();
      runAsyncAction(async () => {
        try {
          const pairs = await tauriApi.getChainSwapPairs();
          state.sendBtcPairInfo = pairs.liquidToBitcoin;
        } catch {
          /* ignore */
        }
        render();
      });
      return;
    }

    if (action === "close-modal") {
      state.walletModal = "none";
      resetReceiveState();
      resetSendState();
      render();
      return;
    }

    if (action === "modal-backdrop" && actionEl === target) {
      state.walletModal = "none";
      resetReceiveState();
      resetSendState();
      render();
      return;
    }

    if (action === "modal-tab") {
      const tab = actionEl?.getAttribute("data-tab-value") as
        | "lightning"
        | "liquid"
        | "bitcoin"
        | null;
      if (tab) {
        state.walletModalTab = tab;
        state.modalQr = "";
        render();
      }
      return;
    }

    if (action === "receive-preset") {
      const preset = actionEl?.getAttribute("data-preset") ?? "";
      if (preset) {
        state.receiveAmount = preset;
        render();
      }
      return;
    }

    if (action === "create-lightning-receive") {
      const amt = Math.floor(Number(state.receiveAmount) || 0);
      if (amt <= 0) {
        state.receiveError = "Enter a valid amount.";
        render();
        return;
      }
      state.receiveCreating = true;
      state.receiveError = "";
      render();
      runAsyncAction(async () => {
        try {
          const swap = await tauriApi.createLightningReceive(amt);
          state.receiveLightningSwap = swap;
          await generateQr(swap.invoice);
        } catch (e) {
          state.receiveError = String(e);
        }
        state.receiveCreating = false;
        render();
      });
      return;
    }

    if (action === "generate-liquid-address") {
      runAsyncAction(async () => {
        try {
          const addr = await tauriApi.getWalletAddress(
            state.receiveLiquidAddressIndex,
          );
          state.receiveLiquidAddress = addr.address;
          await generateQr(addr.address);
          state.receiveLiquidAddressIndex += 1;
        } catch (e) {
          state.receiveError = String(e);
        }
        render();
      });
      return;
    }

    if (action === "create-bitcoin-receive") {
      const amt = Math.floor(Number(state.receiveAmount) || 0);
      if (amt <= 0) {
        state.receiveError = "Enter a valid amount.";
        render();
        return;
      }
      state.receiveCreating = true;
      state.receiveError = "";
      render();
      runAsyncAction(async () => {
        try {
          const swap = await tauriApi.createBitcoinReceive(amt);
          state.receiveBitcoinSwap = swap;
          const addr = swap.lockupAddress;
          await generateQr(swap.bip21 || addr);
        } catch (e) {
          state.receiveError = String(e);
        }
        state.receiveCreating = false;
        render();
      });
      return;
    }

    if (action === "pay-lightning-invoice") {
      const invoice = state.sendInvoice.trim();
      if (!invoice) {
        state.sendError = "Paste a BOLT11 invoice.";
        render();
        return;
      }
      state.sendCreating = true;
      state.sendError = "";
      render();
      runAsyncAction(async () => {
        try {
          const swap = await tauriApi.payLightningInvoice(invoice);
          state.sentLightningSwap = swap;
        } catch (e) {
          state.sendError = String(e);
        }
        state.sendCreating = false;
        render();
      });
      return;
    }

    if (action === "send-liquid") {
      const address = state.sendLiquidAddress.trim();
      const amountSat = Math.floor(Number(state.sendLiquidAmount) || 0);
      if (!address || amountSat <= 0) {
        state.sendError = "Enter address and amount.";
        render();
        return;
      }
      state.sendCreating = true;
      state.sendError = "";
      render();
      runAsyncAction(async () => {
        try {
          const result = await tauriApi.sendLbtc(address, amountSat);
          state.sentLiquidResult = { txid: result.txid, feeSat: result.feeSat };
        } catch (e) {
          state.sendError = String(e);
        }
        state.sendCreating = false;
        render();
      });
      return;
    }

    if (action === "create-bitcoin-send") {
      const amt = Math.floor(Number(state.sendBtcAmount) || 0);
      if (amt <= 0) {
        state.sendError = "Enter a valid amount.";
        render();
        return;
      }
      state.sendCreating = true;
      state.sendError = "";
      render();
      runAsyncAction(async () => {
        try {
          const swap = await tauriApi.createBitcoinSend(amt);
          state.sentBitcoinSwap = swap;
          const addr = swap.claimLockupAddress;
          await generateQr(swap.bip21 || addr);
        } catch (e) {
          state.sendError = String(e);
        }
        state.sendCreating = false;
        render();
      });
      return;
    }

    if (action === "copy-modal-value") {
      const val = actionEl?.getAttribute("data-copy-value") ?? "";
      if (val) void navigator.clipboard.writeText(val);
      return;
    }

    if (action === "refresh-swap") {
      const swapId = actionEl?.getAttribute("data-swap-id") ?? "";
      if (!swapId) return;
      runAsyncAction(async () => {
        try {
          await tauriApi.refreshPaymentSwapStatus(swapId);
          const swaps = await tauriApi.listPaymentSwaps();
          if (state.walletData) state.walletData.swaps = swaps;
        } catch (e) {
          state.walletError = String(e);
        }
        render();
      });
      return;
    }

    if (action === "copy-mnemonic") {
      void navigator.clipboard.writeText(state.walletMnemonic);
      return;
    }

    if (action === "show-backup") {
      if (state.walletData) {
        state.walletData.showBackup = true;
        state.walletData.backupWords = [];
        state.walletData.backupPassword = "";
      }
      state.walletError = "";
      render();
      return;
    }

    if (action === "hide-backup") {
      if (state.walletData) {
        state.walletData.showBackup = false;
        state.walletData.backupWords = [];
        state.walletData.backupPassword = "";
      }
      render();
      return;
    }

    if (action === "export-backup") {
      if (!state.walletData?.backupPassword) {
        state.walletError = "Password is required to export recovery phrase.";
        render();
        return;
      }
      state.walletLoading = true;
      state.walletError = "";
      showOverlayLoader("Decrypting backup...");
      render();
      runAsyncAction(async () => {
        try {
          const password = state.walletData?.backupPassword ?? "";
          const count = await tauriApi.getMnemonicWordCount(password);
          const words: string[] = [];
          for (let i = 0; i < count; i++) {
            words.push(await tauriApi.getMnemonicWord(password, i));
          }
          if (state.walletData) {
            state.walletData.backupWords = words;
            state.walletData.backedUp = true;
            state.walletData.backupPassword = "";
          }
        } catch (e) {
          state.walletError = String(e);
        }
        state.walletLoading = false;
        hideOverlayLoader();
        render();
      });
      return;
    }

    if (action === "copy-backup-mnemonic") {
      void navigator.clipboard.writeText(
        (state.walletData?.backupWords ?? []).join(" "),
      );
      return;
    }
  }
}
