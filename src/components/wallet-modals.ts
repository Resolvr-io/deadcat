import { state } from "../state.ts";
import type { RelayBackupResult } from "../types.ts";
import { hexToNpub } from "../utils/crypto.ts";

export function renderMnemonicGrid(mnemonic: string): string {
  const words = mnemonic.split(" ");
  return (
    '<div class="grid grid-cols-3 gap-2">' +
    words
      .map(
        (w, i) =>
          '<div class="flex items-baseline gap-2 rounded bg-slate-800 px-3 py-2">' +
          '<span class="text-xs text-slate-500 w-5 text-right shrink-0">' +
          (i + 1) +
          ".</span>" +
          '<span class="mono text-sm text-slate-100 whitespace-nowrap">' +
          w +
          "</span>" +
          "</div>",
      )
      .join("") +
    "</div>"
  );
}

export function renderBackupModal(loading: boolean): string {
  if (!state.walletShowBackup) return "";

  const closeBtn =
    '<button data-action="hide-backup" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">' +
    '<svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>' +
    "</button>";

  let body: string;
  if (state.walletBackupMnemonic) {
    const backupStatus = state.nostrBackupStatus;
    const securityInfoHtml =
      '<details class="group">' +
      '<summary class="cursor-pointer text-xs text-slate-500 hover:text-slate-400 transition select-none">' +
      "Why is this secure?" +
      "</summary>" +
      '<div class="mt-2 space-y-1.5 text-xs text-slate-500">' +
      '<p><strong class="text-slate-400">NIP-44 encryption</strong> &mdash; Your recovery phrase is encrypted using the Nostr NIP-44 protocol (XChaCha20 + secp256k1 ECDH). Only your private key (nsec) can decrypt it.</p>' +
      '<p><strong class="text-slate-400">Self-encrypted</strong> &mdash; The backup is encrypted to your own public key, so no one else can read it &mdash; not even the relay operators.</p>' +
      '<p><strong class="text-slate-400">Stored as NIP-78</strong> &mdash; The encrypted data is published as a kind 30078 addressable event (application-specific data). It can be retrieved from any relay that has it.</p>' +
      '<p><strong class="text-slate-400">Relay redundancy</strong> &mdash; The backup is sent to all your configured relays, so it survives even if some go offline.</p>' +
      "</div>" +
      "</details>";
    const nostrBackupHtml = state.nostrNpub
      ? '<div class="rounded-lg border border-slate-700 bg-slate-900/50 p-3 space-y-2">' +
        '<p class="text-[11px] font-medium uppercase tracking-wider text-slate-500">Nostr Relay Backup</p>' +
        (backupStatus?.has_backup
          ? '<p class="text-xs text-emerald-400">Encrypted backup stored on ' +
            backupStatus.relay_results.filter(
              (r: RelayBackupResult) => r.has_backup,
            ).length +
            " of " +
            backupStatus.relay_results.length +
            " relays</p>"
          : '<p class="text-xs text-slate-400">Encrypt and store your recovery phrase on Nostr relays using NIP-44.</p>' +
            '<button data-action="nostr-backup-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300 transition"' +
            (state.nostrBackupLoading ? " disabled" : "") +
            ">" +
            (state.nostrBackupLoading
              ? "Encrypting..."
              : "Encrypt & Upload to Relays") +
            "</button>") +
        securityInfoHtml +
        "</div>"
      : "";
    body =
      renderMnemonicGrid(state.walletBackupMnemonic) +
      '<div class="flex gap-3">' +
      '<button data-action="copy-backup-mnemonic" class="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Copy to clipboard</button>' +
      '<button data-action="hide-backup" class="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Done</button>' +
      "</div>" +
      nostrBackupHtml;
  } else {
    body =
      '<p class="text-sm text-slate-400">Enter your wallet password to reveal your recovery phrase.</p>' +
      '<input id="wallet-backup-password" type="password" maxlength="32" value="' +
      state.walletBackupPassword +
      '" placeholder="Wallet password" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
      '<button data-action="export-backup" class="w-full rounded-xl bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' +
      (loading ? " disabled" : "") +
      ">Show Recovery Phrase</button>";
  }

  return (
    '<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">' +
    '<div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">' +
    '<div class="flex items-center justify-between">' +
    '<h2 class="text-lg font-medium text-slate-100">Backup Recovery Phrase</h2>' +
    closeBtn +
    "</div>" +
    '<div class="mt-5 space-y-4">' +
    body +
    '<p class="text-xs text-slate-500"><strong class="text-slate-300">Deadcat.live does not hold user funds.</strong> If you lose your recovery phrase and password, your funds cannot be recovered.</p>' +
    "</div>" +
    "</div></div>"
  );
}

export function renderCopyable(
  value: string,
  label: string,
  copyAction: string,
): string {
  return (
    '<div class="flex items-center gap-2">' +
    '<div class="flex-1 overflow-hidden rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">' +
    '<div class="text-xs text-slate-500">' +
    label +
    "</div>" +
    '<div class="mono text-xs text-slate-300 truncate">' +
    value +
    "</div>" +
    "</div>" +
    '<button data-action="' +
    copyAction +
    '" data-copy-value="' +
    value +
    '" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800">Copy</button>' +
    "</div>"
  );
}

export function renderModalTabs(): string {
  const tabs: Array<"lightning" | "liquid" | "bitcoin"> = [
    "lightning",
    "liquid",
    "bitcoin",
  ];
  return (
    '<div class="flex rounded-lg border border-slate-700 bg-slate-900/50 p-1 gap-1">' +
    tabs
      .map((t) => {
        const active = state.walletModalTab === t;
        const label =
          t === "lightning"
            ? "Lightning"
            : t === "liquid"
              ? "Liquid"
              : "Bitcoin";
        return (
          '<button data-action="modal-tab" data-tab-value="' +
          t +
          '" class="flex-1 rounded-md px-3 py-2 text-sm font-semibold transition ' +
          (active
            ? "bg-slate-700 text-slate-100"
            : "text-slate-400 hover:text-slate-200") +
          '">' +
          label +
          "</button>"
        );
      })
      .join("") +
    "</div>"
  );
}

export function renderReceiveModal(): string {
  const err = state.receiveError;
  const creating = state.receiveCreating;

  let content = "";

  if (state.walletModalTab === "lightning") {
    if (state.receiveLightningSwap) {
      const s = state.receiveLightningSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Invoice Ready</p>' +
        '<p class="text-xs text-slate-400">Swap ' +
        s.id.slice(0, 8) +
        "... | " +
        s.expectedOnchainAmountSat.toLocaleString() +
        " sats expected on Liquid</p>" +
        '<p class="text-xs text-slate-500">Expires: ' +
        new Date(s.invoiceExpiresAt).toLocaleString() +
        "</p>" +
        (state.modalQr
          ? '<div class="flex justify-center"><img src="' +
            state.modalQr +
            '" alt="QR" class="w-56 h-56 rounded-lg" /></div>'
          : "") +
        renderCopyable(s.invoice, "BOLT11 Invoice", "copy-modal-value") +
        "</div>";
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create a Lightning invoice via Boltz swap. Funds settle as L-BTC.</p>' +
        '<div class="flex gap-2">' +
        '<button data-action="receive-preset" data-preset="1000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">1k</button>' +
        '<button data-action="receive-preset" data-preset="10000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">10k</button>' +
        '<button data-action="receive-preset" data-preset="100000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">100k</button>' +
        "</div>" +
        '<div class="flex gap-2">' +
        '<input id="receive-amount" type="number" value="' +
        state.receiveAmount +
        '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-lightning-receive" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Creating..." : "Create Invoice") +
        "</button>" +
        "</div>" +
        "</div>";
    }
  } else if (state.walletModalTab === "liquid") {
    if (state.receiveLiquidAddress) {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Send L-BTC to this address to fund your wallet.</p>' +
        (state.modalQr
          ? '<div class="flex justify-center"><img src="' +
            state.modalQr +
            '" alt="QR" class="w-56 h-56 rounded-lg" /></div>'
          : "") +
        renderCopyable(
          state.receiveLiquidAddress,
          "Liquid Address",
          "copy-modal-value",
        ) +
        '<button data-action="generate-liquid-address" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">New Address</button>' +
        "</div>";
    } else {
      content =
        '<div class="flex flex-col items-center gap-4 py-4">' +
        '<p class="text-sm text-slate-400">Generate a Liquid address to receive L-BTC.</p>' +
        '<button data-action="generate-liquid-address" class="rounded-lg bg-emerald-400 px-6 py-3 font-medium text-slate-950 hover:bg-emerald-300">' +
        (creating ? "Generating..." : "Generate Address") +
        "</button>" +
        "</div>";
    }
  } else {
    // Bitcoin tab
    if (state.receiveBitcoinSwap) {
      const s = state.receiveBitcoinSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Bitcoin Deposit Address Ready</p>' +
        '<p class="text-xs text-slate-400">Swap ' +
        s.id.slice(0, 8) +
        "... | " +
        s.expectedAmountSat.toLocaleString() +
        " sats expected on Liquid</p>" +
        '<p class="text-xs text-slate-500">Timeout block: ' +
        s.timeoutBlockHeight +
        "</p>" +
        (state.modalQr
          ? '<div class="flex justify-center"><img src="' +
            state.modalQr +
            '" alt="QR" class="w-56 h-56 rounded-lg" /></div>'
          : "") +
        renderCopyable(
          s.lockupAddress,
          "Bitcoin Lockup Address",
          "copy-modal-value",
        ) +
        (s.bip21
          ? renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value")
          : "") +
        "</div>";
    } else {
      const pair = state.receiveBtcPairInfo;
      const pairInfo = pair
        ? '<div class="rounded-lg border border-slate-700 bg-slate-900 p-3 text-xs text-slate-400 space-y-1">' +
          "<div>Min: " +
          pair.minAmountSat.toLocaleString() +
          " sats</div>" +
          "<div>Max: " +
          pair.maxAmountSat.toLocaleString() +
          " sats</div>" +
          "<div>Fee: " +
          pair.feePercentage +
          "% + " +
          pair.fixedMinerFeeTotalSat.toLocaleString() +
          " sats fixed</div>" +
          "</div>"
        : "";
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create a Boltz chain swap. Send BTC on-chain to receive L-BTC.</p>' +
        pairInfo +
        '<div class="flex gap-2">' +
        '<input id="receive-amount" type="number" value="' +
        state.receiveAmount +
        '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-bitcoin-receive" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Creating..." : "Create Address") +
        "</button>" +
        "</div>" +
        "</div>";
    }
  }

  if (err) {
    content +=
      '<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">' +
      err +
      "</div>";
  }

  return content;
}

export function renderSendModal(): string {
  const err = state.sendError;
  const creating = state.sendCreating;

  let content = "";

  if (state.walletModalTab === "lightning") {
    if (state.sentLightningSwap) {
      const s = state.sentLightningSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Swap Created</p>' +
        '<p class="text-xs text-slate-400">Swap ' +
        s.id.slice(0, 8) +
        "... | " +
        s.invoiceAmountSat.toLocaleString() +
        " sats</p>" +
        '<p class="text-xs text-slate-500">Waiting for lockup confirmation. Expires: ' +
        new Date(s.invoiceExpiresAt).toLocaleString() +
        "</p>" +
        renderCopyable(s.lockupAddress, "Lockup Address", "copy-modal-value") +
        renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value") +
        "</div>";
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Paste a BOLT11 Lightning invoice to pay via Boltz submarine swap.</p>' +
        '<input id="send-invoice" value="' +
        state.sendInvoice +
        '" placeholder="BOLT11 invoice" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="pay-lightning-invoice" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Creating Swap..." : "Pay via Lightning") +
        "</button>" +
        "</div>";
    }
  } else if (state.walletModalTab === "liquid") {
    if (state.sentLiquidResult) {
      const r = state.sentLiquidResult;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-emerald-300">Transaction Sent</p>' +
        '<p class="text-xs text-slate-400">Fee: ' +
        r.feeSat.toLocaleString() +
        " sats</p>" +
        renderCopyable(r.txid, "Transaction ID", "copy-modal-value") +
        "</div>";
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Send L-BTC directly to a Liquid address.</p>' +
        '<input id="send-liquid-address" value="' +
        state.sendLiquidAddress +
        '" placeholder="Liquid address" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<input id="send-liquid-amount" type="number" value="' +
        state.sendLiquidAmount +
        '" placeholder="Amount (sats)" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="send-liquid" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Sending..." : "Send L-BTC") +
        "</button>" +
        "</div>";
    }
  } else {
    // Bitcoin tab
    if (state.sentBitcoinSwap) {
      const s = state.sentBitcoinSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Chain Swap Created</p>' +
        '<p class="text-xs text-slate-400">Swap ' +
        s.id.slice(0, 8) +
        "... | " +
        s.expectedAmountSat.toLocaleString() +
        " sats expected on Bitcoin</p>" +
        '<p class="text-xs text-slate-500">Timeout block: ' +
        s.timeoutBlockHeight +
        "</p>" +
        (state.modalQr
          ? '<div class="flex justify-center"><img src="' +
            state.modalQr +
            '" alt="QR" class="w-56 h-56 rounded-lg" /></div>'
          : "") +
        renderCopyable(
          s.lockupAddress,
          "Liquid Lockup Address",
          "copy-modal-value",
        ) +
        (s.bip21
          ? renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value")
          : "") +
        "</div>";
    } else {
      const pair = state.sendBtcPairInfo;
      const pairInfo = pair
        ? '<div class="rounded-lg border border-slate-700 bg-slate-900 p-3 text-xs text-slate-400 space-y-1">' +
          "<div>Min: " +
          pair.minAmountSat.toLocaleString() +
          " sats</div>" +
          "<div>Max: " +
          pair.maxAmountSat.toLocaleString() +
          " sats</div>" +
          "<div>Fee: " +
          pair.feePercentage +
          "% + " +
          pair.fixedMinerFeeTotalSat.toLocaleString() +
          " sats fixed</div>" +
          "</div>"
        : "";
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create an L-BTC to BTC chain swap via Boltz.</p>' +
        pairInfo +
        '<div class="flex gap-2">' +
        '<input id="send-btc-amount" type="number" value="' +
        state.sendBtcAmount +
        '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-bitcoin-send" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Creating..." : "Create Swap") +
        "</button>" +
        "</div>" +
        "</div>";
    }
  }

  if (err) {
    content +=
      '<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">' +
      err +
      "</div>";
  }

  return content;
}

export function renderNostrEventModal(): string {
  if (!state.nostrEventModal || !state.nostrEventJson) return "";

  const esc = (s: string) =>
    s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");

  let parsed: {
    id?: string;
    pubkey?: string;
    kind?: number;
    created_at?: number;
    tags?: string[][];
    content?: string;
    sig?: string;
  } | null = null;
  try {
    parsed = JSON.parse(state.nostrEventJson);
  } catch {
    /* raw fallback */
  }

  let contentPretty: string | null = null;
  if (parsed?.content) {
    try {
      contentPretty = JSON.stringify(JSON.parse(parsed.content), null, 2);
    } catch {
      contentPretty = parsed.content;
    }
  }

  const truncHex = (v: string) =>
    v.length > 16 ? `${v.slice(0, 8)}\u2026${v.slice(-8)}` : v;
  const truncBech32 = (v: string) =>
    v.length > 24 ? `${v.slice(0, 12)}\u2026${v.slice(-8)}` : v;

  const fieldHtml = (
    label: string,
    value: string | undefined,
    opts?: { copyable?: boolean; displayValue?: string },
  ) => {
    if (!value) return "";
    const copyable = opts?.copyable ?? false;
    const display = opts?.displayValue
      ? truncBech32(opts.displayValue)
      : copyable
        ? truncHex(value)
        : esc(value);
    const copyVal = opts?.displayValue ?? value;
    return `<div class="min-w-0">
      <span class="mb-0.5 block text-xs text-slate-500">${label}</span>
      ${
        copyable
          ? `<button data-action="copy-to-clipboard" data-copy-value="${esc(copyVal)}" class="flex w-full min-w-0 items-center gap-1.5 overflow-hidden font-mono text-xs text-slate-200 transition-colors hover:text-white">
            <span class="truncate">${display}</span>
            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0 text-slate-500"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
          </button>`
          : `<p class="truncate text-xs text-slate-200">${display}</p>`
      }
    </div>`;
  };

  let fieldsHtml = "";
  if (parsed) {
    const neventDisplay = state.nostrEventNevent ?? parsed.id;
    const npubDisplay = parsed.pubkey ? hexToNpub(parsed.pubkey) : undefined;
    fieldsHtml += fieldHtml("Event ID", parsed.id, {
      copyable: true,
      displayValue: neventDisplay,
    });
    fieldsHtml += fieldHtml("Public Key", parsed.pubkey, {
      copyable: true,
      displayValue: npubDisplay,
    });

    fieldsHtml += `<div class="grid grid-cols-2 gap-3">`;
    fieldsHtml += fieldHtml("Kind", parsed.kind?.toString());
    fieldsHtml += fieldHtml(
      "Created",
      parsed.created_at
        ? new Date(parsed.created_at * 1000).toLocaleString()
        : undefined,
    );
    fieldsHtml += `</div>`;

    if (state.relays.length > 0) {
      let relayPills = "";
      for (const relay of state.relays) {
        const display = relay.url.replace(/^wss?:\/\//, "");
        relayPills += `<span class="inline-flex items-center gap-1 rounded bg-slate-800/80 px-1.5 py-0.5 text-[10px] text-slate-300"><span class="h-1 w-1 rounded-full bg-emerald-400"></span>${esc(display)}</span>`;
      }
      fieldsHtml += `<div><span class="mb-0.5 block text-xs text-slate-500">Relays</span><div class="flex flex-wrap gap-1">${relayPills}</div></div>`;
    }

    if (parsed.tags && parsed.tags.length > 0) {
      fieldsHtml += `<div><span class="mb-1 block text-xs text-slate-500">Tags</span><div class="flex flex-wrap gap-1.5">`;
      for (const tag of parsed.tags) {
        fieldsHtml += `<span class="inline-flex items-center gap-1 rounded-md bg-slate-800 px-2 py-1 font-mono text-[10px] text-slate-200"><span class="font-semibold text-violet-400">${esc(tag[0])}</span>`;
        for (let j = 1; j < tag.length; j++) {
          fieldsHtml += `<span class="max-w-[200px] truncate">${esc(tag[j])}</span>`;
        }
        fieldsHtml += `</span>`;
      }
      fieldsHtml += `</div></div>`;
    }

    fieldsHtml += fieldHtml("Signature", parsed.sig, { copyable: true });

    if (contentPretty) {
      fieldsHtml += `<details><summary class="cursor-pointer text-xs text-slate-500 hover:text-slate-300">Content <span class="text-slate-600">— click to expand</span></summary><pre class="mt-1 max-h-48 overflow-auto rounded-lg bg-slate-800 p-3 font-mono text-[11px] text-slate-200 leading-relaxed">${esc(contentPretty)}</pre></details>`;
    }
  } else {
    fieldsHtml = `<pre class="max-h-96 overflow-auto rounded-lg bg-slate-800 p-3 font-mono text-[11px] text-slate-200 leading-relaxed">${esc(state.nostrEventJson)}</pre>`;
  }

  return `
    <div data-action="nostr-event-backdrop" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div class="relative mx-4 w-full max-w-md rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div class="flex items-center justify-between border-b border-slate-800 px-6 py-4">
          <div class="flex items-center gap-2.5">
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-violet-500/15">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-violet-400"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>
            </div>
            <h3 class="text-lg font-medium text-slate-100">Nostr Event</h3>
          </div>
          <button data-action="close-nostr-event-modal" class="rounded-lg p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-200">
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        </div>
        <div class="flex flex-col gap-3 p-6 max-h-[70vh] overflow-y-auto">
          ${fieldsHtml}
        </div>
        <div class="flex items-center justify-end gap-2 border-t border-slate-800 px-6 py-4">
          ${
            state.nostrEventNevent
              ? `<button data-action="copy-to-clipboard" data-copy-value="${state.nostrEventNevent}" class="flex items-center gap-1.5 rounded-lg border border-slate-700 px-3 py-1.5 text-sm text-slate-200 hover:bg-slate-800">
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
            Copy Event ID
          </button>`
              : ""
          }
          <button data-action="copy-nostr-event-json" class="flex items-center gap-1.5 rounded-lg border border-slate-700 px-3 py-1.5 text-sm text-slate-200 hover:bg-slate-800">
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
            Copy JSON
          </button>
          <button data-action="close-nostr-event-modal" class="rounded-lg bg-slate-800 px-4 py-1.5 text-sm text-slate-200 hover:bg-slate-700">Close</button>
        </div>
      </div>
    </div>
  `;
}
