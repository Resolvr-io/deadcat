import { renderBackupModal } from "../components/wallet-modals.ts";
import { formatCompactSats } from "../services/wallet.ts";
import { baseCurrencyOptions, categories, DEV_MODE, state } from "../state.ts";
import type { RelayBackupResult, RelayEntry } from "../types.ts";

export function settingsAccordion(
  key: string,
  title: string,
  content: string,
): string {
  const open = state.settingsSection[key];
  return `<div class="rounded-lg border border-slate-800 overflow-hidden">
    <button data-action="toggle-settings-section" data-section="${key}" class="w-full flex items-center justify-between px-4 py-3 text-left transition-colors hover:bg-slate-900/50">
      <span class="text-xs font-medium uppercase tracking-wider text-slate-400">${title}</span>
      <svg class="h-4 w-4 text-slate-500 transition-transform duration-200 ${open ? "rotate-180" : ""}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/></svg>
    </button>
    ${open ? `<div class="px-4 pb-4 pt-3 border-t border-slate-800">${content}</div>` : ""}
  </div>`;
}

export function renderTopShell(): string {
  return `
    <header class="relative z-30 border-b border-slate-800 bg-slate-950/80 backdrop-blur">
      <div class="phi-container py-4 lg:py-5">
        <div class="flex items-end gap-5">
          <button data-action="go-home" class="shrink-0 py-1 leading-none"><svg class="h-10" viewBox="0 0 1274 267" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M0.146484 9.04596C0.146625 1.23444 10.9146 -3.15996 16.7881 2.6983L86.5566 71.7335C100.141 68.0293 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0305 173.453 71.7364L243.212 2.71197C249.085 -3.1467 259.853 1.24702 259.854 9.05865V161.26C259.949 162.835 260 164.419 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C4.78859e-06 164.419 0.0506708 162.835 0.146484 161.26V9.04596ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.23 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.216 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z" fill="#34D399"/><path d="M861.074 187V124.998H835.164V107.8H906.444V124.998H880.534V187H861.074Z" fill="#34D399"/><path d="M756.185 187L788.657 107.8H810.946L842.965 187H821.921L814.68 167.879H783.792L776.437 187H756.185ZM789.675 152.378H808.909L799.405 127.034L789.675 152.378Z" fill="#34D399"/><path d="M716.605 188.131C710.571 188.131 704.951 187.113 699.747 185.077C694.618 182.965 690.13 180.061 686.283 176.365C682.436 172.669 679.419 168.369 677.231 163.466C675.119 158.488 674.063 153.133 674.063 147.4C674.063 141.592 675.119 136.237 677.231 131.334C679.419 126.355 682.436 122.018 686.283 118.322C690.205 114.626 694.731 111.76 699.86 109.723C705.065 107.611 710.646 106.555 716.605 106.555C720.98 106.555 725.279 107.197 729.503 108.479C733.727 109.761 737.612 111.571 741.157 113.91C744.778 116.173 747.795 118.888 750.209 122.056L737.084 134.954C734.293 131.409 731.163 128.769 727.693 127.034C724.299 125.299 720.603 124.432 716.605 124.432C713.437 124.432 710.458 125.035 707.667 126.242C704.952 127.374 702.575 128.958 700.539 130.994C698.502 133.031 696.918 135.445 695.787 138.235C694.655 141.026 694.09 144.081 694.09 147.4C694.09 150.643 694.655 153.661 695.787 156.451C696.994 159.167 698.615 161.581 700.652 163.693C702.764 165.729 705.215 167.313 708.006 168.445C710.873 169.576 713.965 170.142 717.284 170.142C721.131 170.142 724.676 169.312 727.919 167.653C731.238 165.993 734.218 163.542 736.858 160.298L749.643 172.857C747.229 175.95 744.25 178.665 740.705 181.003C737.16 183.266 733.313 185.039 729.164 186.321C725.015 187.528 720.829 188.131 716.605 188.131Z" fill="#34D399"/><path d="M606.391 169.802H618.61C621.703 169.802 624.569 169.237 627.209 168.105C629.924 166.974 632.3 165.39 634.337 163.353C636.374 161.317 637.958 158.978 639.089 156.338C640.22 153.623 640.786 150.719 640.786 147.626C640.786 144.458 640.22 141.517 639.089 138.801C637.958 136.01 636.374 133.597 634.337 131.56C632.3 129.523 629.924 127.939 627.209 126.808C624.569 125.601 621.703 124.998 618.61 124.998H606.391V169.802ZM586.93 187V107.8H619.063C624.946 107.8 630.415 108.818 635.468 110.855C640.522 112.891 644.935 115.72 648.706 119.341C652.553 122.961 655.532 127.185 657.644 132.013C659.832 136.84 660.926 142.045 660.926 147.626C660.926 153.133 659.832 158.262 657.644 163.014C655.532 167.766 652.553 171.952 648.706 175.573C644.935 179.118 640.522 181.909 635.468 183.945C630.415 185.982 624.946 187 619.063 187H586.93Z" fill="#34D399"/><path d="M488.73 187L521.202 107.8H543.491L575.511 187H554.466L547.225 167.879H516.337L508.983 187H488.73ZM522.22 152.378H541.455L531.951 127.034L522.22 152.378Z" fill="#34D399"/><path d="M414.962 187V107.8H477.417V124.658H434.422V138.914H462.821V155.207H434.422V170.142H477.869V187H414.962Z" fill="#34D399"/><path d="M344.461 169.802H356.68C359.773 169.802 362.639 169.237 365.279 168.105C367.994 166.974 370.37 165.39 372.407 163.353C374.443 161.317 376.027 158.978 377.159 156.338C378.29 153.623 378.856 150.719 378.856 147.626C378.856 144.458 378.29 141.517 377.159 138.801C376.027 136.01 374.443 133.597 372.407 131.56C370.37 129.523 367.994 127.939 365.279 126.808C362.639 125.601 359.773 124.998 356.68 124.998H344.461V169.802ZM325 187V107.8H357.133C363.016 107.8 368.485 108.818 373.538 110.855C378.592 112.891 383.005 115.72 386.776 119.341C390.623 122.961 393.602 127.185 395.714 132.013C397.902 136.84 398.995 142.045 398.995 147.626C398.995 153.133 397.902 158.262 395.714 163.014C393.602 167.766 390.623 171.952 386.776 175.573C383.005 179.118 378.592 181.909 373.538 183.945C368.485 185.982 363.016 187 357.133 187H325Z" fill="#34D399"/><rect x="937.564" y="67.8096" width="336.286" height="152.952" rx="26.1905" fill="#EF4444"/><path d="M1178.21 187V107.8H1240.66V124.658H1197.67V138.914H1226.07V155.207H1197.67V170.142H1241.11V187H1178.21Z" fill="white"/><path d="M1112.03 187L1080.01 107.8H1101.05L1123.57 166.747L1146.54 107.8H1166.79L1134.32 187H1112.03Z" fill="white"/><path d="M1049.11 187V107.8H1068.57V187H1049.11Z" fill="white"/><path d="M971.412 187V107.8H990.873V169.802H1032.62V187H971.412Z" fill="white"/></svg></button>
          <nav class="flex shrink-0 items-baseline gap-5 whitespace-nowrap pb-[9px] text-base text-slate-400">
            <button data-action="go-home" class="${state.view === "home" || state.view === "detail" ? "font-medium text-slate-100" : "hover:text-slate-200"}">Markets</button>
            <button class="hover:text-slate-200">Live</button>
            <button class="hover:text-slate-200">Social</button>
          </nav>
          <div class="ml-auto flex shrink-0 items-center gap-2 pb-[5px]">
            <input id="global-search" value="${state.search}" class="hidden h-9 w-[280px] rounded-full border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-300 transition focus:ring-2 lg:block xl:w-[380px]" placeholder="Trade on anything" />
            <button data-action="open-search" class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200 lg:hidden">
              <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
            </button>
            <button data-action="open-wallet" class="flex h-9 shrink-0 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200 ${state.showMiniWallet && state.walletStatus === "unlocked" && state.walletBalance && !state.walletBalanceHidden ? "gap-1.5 px-3" : "w-9"}">
              <svg class="h-[18px] w-[18px] shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="4" width="22" height="16" rx="2" ry="2"/><line x1="1" y1="10" x2="23" y2="10"/></svg>
              ${state.showMiniWallet && state.walletStatus === "unlocked" && state.walletBalance && !state.walletBalanceHidden ? `<span class="text-xs font-medium text-slate-300">${formatCompactSats(state.walletBalance[state.walletPolicyAssetId] ?? 0)}</span>` : ""}
            </button>
            <div class="relative shrink-0">
              <button data-action="toggle-user-menu" class="flex h-9 w-9 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200 overflow-hidden">
                ${
                  state.nostrProfile?.picture && !state.profilePicError
                    ? `<img src="${state.nostrProfile.picture}" class="h-full w-full rounded-full object-cover" onerror="this.style.display='none';this.nextElementSibling.style.display='block'" /><svg style="display:none" class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`
                    : `<svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`
                }
              </button>
              ${
                state.userMenuOpen
                  ? `<div class="absolute right-0 top-full z-50 mt-2 w-64 rounded-xl border border-slate-700 bg-slate-900 shadow-xl">
                ${
                  state.nostrNpub
                    ? `<div class="px-3 pb-1 pt-3">
                  <div class="mb-1.5 text-[11px] text-slate-500">Nostr Publishing ID</div>
                  <button data-action="copy-nostr-npub" class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-slate-800" title="Click to copy npub">
                    <span class="mono min-w-0 truncate text-xs text-slate-300">${state.nostrNpub}</span>
                    <svg class="h-3.5 w-3.5 shrink-0 text-slate-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                  </button>
                </div>`
                    : ""
                }
                <div class="px-3 pb-1 ${state.nostrNpub ? "pt-1 border-t border-slate-800" : "pt-3"}">
                  <div class="mb-1.5 text-[11px] text-slate-500">Display currency</div>
                  <div class="grid grid-cols-3 gap-1">
                    ${baseCurrencyOptions.map((c) => `<button data-action="set-currency" data-currency="${c}" class="rounded-md px-2 py-1 text-xs transition ${c === state.baseCurrency ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:bg-slate-800 hover:text-slate-200"}">${c}</button>`).join("")}
                  </div>
                </div>
                <div class="mt-1 border-t border-slate-800 py-1">
                  <button data-action="user-settings" class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-slate-300 hover:bg-slate-800 hover:text-slate-100">
                    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
                    Settings
                  </button>
                </div>
                <div class="border-t border-slate-800 py-1">
                  <button data-action="user-logout" class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-slate-300 hover:bg-slate-800 hover:text-slate-100">
                    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>
                    Log out
                  </button>
                </div>
              </div>`
                  : ""
              }
            </div>
          </div>
        </div>
      </div>
      <div class="border-t border-slate-800">
        <div class="phi-container py-2">
          <div id="category-row" class="flex items-center gap-1 overflow-x-auto whitespace-nowrap">
            ${categories
              .filter(
                (category) => category !== "My Markets" || state.nostrPubkey,
              )
              .map((category) => {
                const active = state.activeCategory === category;
                return `<button data-category="${category}" class="rounded-full px-3 py-1.5 text-sm font-normal transition ${
                  active
                    ? "bg-slate-800/80 text-slate-100"
                    : "text-slate-500 hover:text-slate-300"
                }">${category}</button>`;
              })
              .join("")}
            <button data-action="open-help" class="ml-auto flex shrink-0 items-center gap-1.5 rounded-full px-3 py-1.5 text-sm font-normal text-slate-500 transition hover:text-slate-300">
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 11h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-5Zm0 0a9 9 0 1 1 18 0m0 0v5a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3Z"/><path d="M21 16v2a4 4 0 0 1-4 4h-5"/></svg>
              Help
            </button>
          </div>
        </div>
      </div>
    </header>
    ${
      state.searchOpen
        ? `<div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm lg:hidden">
      <div class="flex items-center gap-3 border-b border-slate-800 bg-slate-950 px-4 py-3">
        <input id="global-search-mobile" value="${state.search}" class="h-10 flex-1 rounded-full border border-slate-700 bg-slate-900 px-4 text-sm text-slate-200 outline-none ring-emerald-300 transition focus:ring-2" placeholder="Trade on anything" autofocus />
        <button data-action="close-search" class="shrink-0 text-sm text-slate-400 hover:text-slate-200">Cancel</button>
      </div>
    </div>`
        : ""
    }
    ${
      state.helpOpen
        ? `<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-slate-100">Help</h2>
          <button data-action="close-help" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <p class="mt-4 text-sm text-slate-400">Help content coming soon.</p>
      </div>
    </div>`
        : ""
    }
    ${
      state.settingsOpen
        ? `<div class="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-slate-950/80 backdrop-blur-sm py-8">
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8 my-auto">
        ${
          state.nostrReplacePanel
            ? `
        <div class="flex items-center justify-between">
          <button data-action="nostr-replace-back" class="flex items-center gap-1 text-sm text-slate-400 hover:text-slate-200 transition">
            <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7"/></svg>
            Back
          </button>
          <button data-action="close-settings" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <div class="mt-5 space-y-5">
          <div>
            <p class="text-xs font-medium uppercase tracking-wider text-slate-500">${state.nostrNpub ? "Replace Nostr Keys" : "Set Up Nostr Identity"}</p>
            <p class="mt-1 text-xs text-slate-400">${state.nostrNpub ? "Your current identity will be permanently deleted. Choose how to set up your new identity." : "Import an existing key or generate a new one."}</p>
          </div>
          <div class="space-y-3">
            <p class="text-[10px] font-medium uppercase tracking-wider text-slate-500">Import existing nsec</p>
            <div class="flex items-center gap-2">
              <input id="nostr-import-nsec" type="password" value="${state.nostrImportNsec}" placeholder="nsec1..." class="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono" />
              <button data-action="import-nostr-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${state.nostrImporting ? "disabled" : ""}>${state.nostrImporting ? "Importing..." : "Import"}</button>
            </div>
          </div>
          <div class="border-t border-slate-800 pt-4 space-y-3">
            <p class="text-[10px] font-medium uppercase tracking-wider text-slate-500">Or generate a fresh keypair</p>
            <button data-action="generate-new-nostr-key" class="w-full rounded-lg bg-emerald-400 px-4 py-2.5 text-sm font-medium text-slate-950 hover:bg-emerald-300 transition">Generate New Keypair</button>
          </div>
        </div>
        `
            : `
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-slate-100">Settings</h2>
          <button data-action="close-settings" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <div class="mt-3 space-y-2">
          ${settingsAccordion(
            "nostr",
            "Nostr Identity",
            `
            <div class="space-y-3">
              <p class="text-xs text-slate-500">Used to publish markets and oracle attestations on Nostr.</p>
              <div class="space-y-2">
                <div class="flex items-center gap-2">
                  <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
                    <div class="text-[10px] text-slate-500">npub (public)</div>
                    <div class="mono truncate text-xs text-slate-300">${state.nostrNpub ?? "Not initialized"}</div>
                  </div>
                  ${state.nostrNpub ? `<button data-action="copy-nostr-npub" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>` : ""}
                </div>
                ${
                  state.nostrNpub
                    ? `<div class="flex items-center gap-2">
                  <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
                    <div class="text-[10px] text-slate-500">nsec (secret)</div>
                    ${
                      state.nostrNsecRevealed
                        ? `<div class="mono truncate text-xs text-rose-300">${state.nostrNsecRevealed}</div>`
                        : `<div class="text-xs text-slate-500">Hidden</div>`
                    }
                  </div>
                  ${
                    state.nostrNsecRevealed
                      ? `<button data-action="copy-nostr-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>`
                      : `<button data-action="reveal-nostr-nsec" class="shrink-0 rounded-lg border border-amber-700/60 bg-amber-950/20 px-3 py-2 text-xs text-amber-300 hover:bg-amber-900/30 transition">Reveal</button>`
                  }
                </div>`
                    : ""
                }
              </div>
              ${
                state.nostrNpub
                  ? `<div class="rounded-lg border border-amber-700/40 bg-amber-950/20 px-3 py-2">
                <p class="text-[11px] text-amber-300/90">Back up your nsec in a safe place — if lost, you cannot resolve markets you created.</p>
              </div>`
                  : ""
              }
              ${
                !state.nostrNpub
                  ? `<div class="space-y-3">
                    <div>
                      <p class="text-[10px] font-medium uppercase tracking-wider text-slate-500">Import existing nsec</p>
                      <div class="mt-1 flex items-center gap-2">
                        <input id="nostr-import-nsec" type="password" value="${state.nostrImportNsec}" placeholder="nsec1..." class="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono" />
                        <button data-action="import-nostr-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${state.nostrImporting ? "disabled" : ""}>${state.nostrImporting ? "Importing..." : "Import"}</button>
                      </div>
                    </div>
                    <div class="border-t border-slate-800 pt-3">
                      <p class="text-[10px] font-medium uppercase tracking-wider text-slate-500">Or generate a fresh keypair</p>
                      <button data-action="generate-new-nostr-key" class="mt-1 w-full rounded-lg bg-emerald-400 px-4 py-2.5 text-sm font-medium text-slate-950 hover:bg-emerald-300 transition">Generate New Keypair</button>
                    </div>
                  </div>`
                  : state.nostrReplacePrompt
                    ? `<div class="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
                      <p class="text-[11px] text-rose-300">This will permanently erase your current Nostr identity. Type <strong>DELETE</strong> to confirm.</p>
                      <div class="flex items-center gap-2">
                        <input id="nostr-replace-confirm" type="text" value="${state.nostrReplaceConfirm}" placeholder="Type DELETE" class="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase" autocomplete="off" />
                        <button data-action="nostr-replace-confirm" class="shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${state.nostrReplaceConfirm.trim().toUpperCase() === "DELETE" ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}" ${state.nostrReplaceConfirm.trim().toUpperCase() !== "DELETE" ? "disabled" : ""}>Continue</button>
                        <button data-action="nostr-replace-cancel" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                      </div>
                    </div>`
                    : `<button data-action="nostr-replace-start" class="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition">Replace Nostr Keys</button>`
              }
            </div>
          `,
          )}
          ${settingsAccordion(
            "wallet",
            "Wallet",
            `
            <div class="space-y-3">
              ${
                state.walletStatus === "not_created"
                  ? `<p class="text-xs text-slate-500">No wallet configured on this device.</p>
                   <button data-action="open-wallet" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Set Up Wallet</button>`
                  : `<div class="flex items-center justify-between rounded-lg border border-slate-700 bg-slate-900/50 px-3 py-2.5">
                  <div>
                    <p class="text-xs text-slate-300">Show balance in nav bar</p>
                    <p class="text-[10px] text-slate-500">Display mini wallet balance next to the wallet icon</p>
                  </div>
                  <button data-action="toggle-mini-wallet" class="relative h-5 w-9 rounded-full transition ${state.showMiniWallet ? "bg-emerald-400" : "bg-slate-700"}">
                    <span class="absolute top-0.5 ${state.showMiniWallet ? "left-[18px]" : "left-0.5"} h-4 w-4 rounded-full bg-white shadow transition-all"></span>
                  </button>
                </div>
                <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-3 space-y-2">
                  <p class="text-[11px] font-medium uppercase tracking-wider text-slate-500">Display Currency</p>
                  <p class="text-[10px] text-slate-500">Show fiat equivalents for BTC amounts</p>
                  <div class="grid grid-cols-3 gap-1">
                    ${baseCurrencyOptions.map((c) => `<button data-action="set-currency" data-currency="${c}" class="rounded-md px-2 py-1 text-xs transition ${c === state.baseCurrency ? "bg-emerald-400/15 border border-emerald-400/40 text-emerald-300" : "border border-slate-700 text-slate-400 hover:bg-slate-800 hover:text-slate-200"}">${c}</button>`).join("")}
                  </div>
                </div>
                ${
                      state.nostrNpub
                        ? `<div class="rounded-lg border border-slate-700 bg-slate-900/50 p-3 space-y-2">
                  <p class="text-[11px] font-medium uppercase tracking-wider text-slate-500">Nostr Relay Backup</p>
                  ${
                    state.nostrBackupStatus?.has_backup
                      ? `<div class="flex items-center gap-2">
                        <svg class="h-4 w-4 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
                        <p class="text-xs text-emerald-400">Encrypted backup on ${state.nostrBackupStatus.relay_results.filter((r: RelayBackupResult) => r.has_backup).length} of ${state.nostrBackupStatus.relay_results.length} relays</p>
                      </div>
                      <div class="space-y-1">
                        ${state.nostrBackupStatus.relay_results
                          .map(
                            (
                              r: RelayBackupResult,
                            ) => `<div class="flex items-center gap-2 text-xs">
                          <span class="h-1.5 w-1.5 rounded-full ${r.has_backup ? "bg-emerald-400" : "bg-slate-600"}"></span>
                          <span class="mono text-slate-400">${r.url}</span>
                        </div>`,
                          )
                          .join("")}
                      </div>
                      ${
                        state.nostrBackupPrompt &&
                        state.walletStatus !== "unlocked"
                          ? `<div class="space-y-2">
                            <input id="settings-backup-password" type="password" maxlength="32" value="${state.nostrBackupPassword}" placeholder="Wallet password" class="h-9 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2" />
                            <div class="flex gap-2">
                              <button data-action="settings-backup-wallet" class="flex-1 rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${state.nostrBackupLoading ? "disabled" : ""}>${state.nostrBackupLoading ? "Uploading..." : "Upload"}</button>
                              <button data-action="cancel-backup-prompt" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                            </div>
                          </div>`
                          : `<div class="flex gap-2">
                            <button data-action="settings-backup-wallet" class="flex-1 rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${state.nostrBackupLoading ? "disabled" : ""}>${state.nostrBackupLoading ? "Uploading..." : "Re-upload to Relays"}</button>
                            <button data-action="delete-nostr-backup" class="shrink-0 rounded-lg border border-rose-700/40 px-3 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition" ${state.nostrBackupLoading ? "disabled" : ""}>Delete</button>
                          </div>`
                      }`
                      : `<p class="text-xs text-slate-400">Encrypt your recovery phrase with NIP-44 and store it on your Nostr relays. Only your nsec can decrypt it.</p>
                      ${
                        state.nostrBackupPrompt &&
                        state.walletStatus !== "unlocked"
                          ? `<div class="space-y-2">
                            <input id="settings-backup-password" type="password" maxlength="32" value="${state.nostrBackupPassword}" placeholder="Wallet password" class="h-9 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2" />
                            <div class="flex gap-2">
                              <button data-action="settings-backup-wallet" class="flex-1 rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${state.nostrBackupLoading ? "disabled" : ""}>${state.nostrBackupLoading ? "Encrypting..." : "Upload"}</button>
                              <button data-action="cancel-backup-prompt" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                            </div>
                          </div>`
                          : `<button data-action="settings-backup-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${state.nostrBackupLoading ? "disabled" : ""}>${state.nostrBackupLoading ? "Encrypting..." : "Encrypt & Upload to Relays"}</button>`
                      }`
                  }
                  <details class="group">
                    <summary class="cursor-pointer text-[11px] text-slate-500 hover:text-slate-400 transition select-none">Why is this secure?</summary>
                    <div class="mt-2 space-y-1.5 text-[11px] text-slate-500">
                      <p><strong class="text-slate-400">NIP-44 encryption</strong> — Recovery phrase is encrypted using XChaCha20 + secp256k1 ECDH. Only your nsec can decrypt it.</p>
                      <p><strong class="text-slate-400">Self-encrypted</strong> — Encrypted to your own public key. Relay operators see only ciphertext.</p>
                      <p><strong class="text-slate-400">NIP-78 storage</strong> — Published as a kind 30078 addressable event, retrievable from any relay that has it.</p>
                      <p><strong class="text-slate-400">Relay redundancy</strong> — Sent to all your configured relays for resilience.</p>
                    </div>
                  </details>
                </div>`
                        : ""
                    }
                  <p class="text-xs text-slate-500">Remove the current wallet from this device. You can restore from a recovery phrase${state.nostrNpub ? " or Nostr backup" : ""}.</p>
                  ${
                    state.walletDeletePrompt
                      ? `<div class="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
                        <p class="text-[11px] text-rose-300">This will permanently remove your wallet. Type <strong>DELETE</strong> to confirm.</p>
                        <div class="flex items-center gap-2">
                          <input id="wallet-delete-confirm" type="text" value="${state.walletDeleteConfirm}" placeholder="Type DELETE" class="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase" autocomplete="off" />
                          <button data-action="wallet-delete-confirm" class="shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${state.walletDeleteConfirm.trim().toUpperCase() === "DELETE" ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}" ${state.walletDeleteConfirm.trim().toUpperCase() !== "DELETE" ? "disabled" : ""}>Continue</button>
                          <button data-action="wallet-delete-cancel" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                        </div>
                      </div>`
                      : `<button data-action="wallet-delete-start" class="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition">Remove Wallet</button>`
                  }`
              }
            </div>
          `,
          )}
          ${settingsAccordion(
            "relays",
            "Relays",
            `
            <div class="space-y-3">
              <p class="text-xs text-slate-500">Nostr relays used for publishing and fetching data.</p>
              <div class="space-y-1.5">
                ${state.relays
                  .map(
                    (
                      relay: RelayEntry,
                    ) => `<div class="flex items-center gap-2 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
                  <div class="min-w-0 flex-1 truncate text-xs text-slate-300 mono">${relay.url}</div>
                  ${relay.has_backup ? `<svg class="h-3.5 w-3.5 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>` : ""}
                  ${
                    state.relays.length > 1
                      ? `<button data-action="remove-relay" data-relay="${relay.url}" class="shrink-0 text-slate-500 hover:text-rose-400 transition">
                    <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                  </button>`
                      : ""
                  }
                </div>`,
                  )
                  .join("")}
              </div>
              <div class="flex items-center gap-2">
                <input id="relay-input" value="${state.relayInput}" placeholder="wss://relay.example.com" class="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono" />
                <button data-action="add-relay" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${state.relayLoading ? "disabled" : ""}>Add</button>
              </div>
              <button data-action="reset-relays" class="text-[10px] text-slate-500 hover:text-slate-300 transition">Reset to defaults</button>
            </div>
          `,
          )}
          ${
            DEV_MODE
              ? settingsAccordion(
                  "dev",
                  "Dev",
                  `
            <div class="space-y-2">
              <button data-action="dev-restart" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Restart App</button>
              ${
                state.devResetPrompt
                  ? `<div class="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
                    <p class="text-[11px] text-rose-300">This will erase your <strong>Nostr identity</strong> and <strong>wallet</strong>. Type <strong>RESET</strong> to confirm.</p>
                    <div class="flex items-center gap-2">
                      <input id="dev-reset-confirm" type="text" value="${state.devResetConfirm}" placeholder="Type RESET" class="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase" autocomplete="off" />
                      <button data-action="dev-reset-confirm" class="shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${state.devResetConfirm.trim().toUpperCase() === "RESET" ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}" ${state.devResetConfirm.trim().toUpperCase() !== "RESET" ? "disabled" : ""}>Confirm</button>
                      <button data-action="dev-reset-cancel" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                    </div>
                  </div>`
                  : `<button data-action="dev-reset-start" class="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition">Erase All App Data</button>`
              }
            </div>
          `,
                )
              : ""
          }
        </div>
        `
        }
      </div>
    </div>`
        : ""
    }
    ${
      state.logoutOpen
        ? `<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div class="w-full max-w-md rounded-2xl border border-slate-800 bg-slate-950 p-8">
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-slate-100">Log Out</h2>
          <button data-action="close-logout" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <div class="mt-5 space-y-4">
          <div class="rounded-xl border border-slate-700 bg-slate-900/60 p-4">
            <p class="text-sm font-medium text-slate-200">Before you log out, make sure you have:</p>
            <ul class="mt-3 space-y-2 text-sm text-slate-400">
              <li class="flex items-start gap-2">
                <svg class="mt-0.5 h-4 w-4 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/></svg>
                <span>Backed up your <strong class="text-slate-200">recovery phrase</strong> — this is the only way to restore your wallet</span>
              </li>
              <li class="flex items-start gap-2">
                <svg class="mt-0.5 h-4 w-4 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/></svg>
                <span>Saved your <strong class="text-slate-200">unlock password</strong> — you'll need it to access your wallet again</span>
              </li>
            </ul>
          </div>
          <p class="text-xs text-slate-500"><strong class="text-slate-300">Deadcat.live does not hold user funds.</strong> If you lose your recovery phrase and password, your funds cannot be recovered.</p>
          <div class="flex gap-3">
            <button data-action="close-logout" class="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Cancel</button>
            <button data-action="confirm-logout" class="flex-1 rounded-xl bg-rose-500/20 py-2.5 text-sm font-medium text-rose-300 transition hover:bg-rose-500/30">Log Out</button>
          </div>
        </div>
      </div>
    </div>`
        : ""
    }
    ${renderBackupModal(state.walletLoading)}
  `;
}
