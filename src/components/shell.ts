import { renderBackupModal } from "../components/wallet-modals.ts";
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
          <button data-action="go-home" class="shrink-0 py-1 leading-none"><svg class="h-10" viewBox="0 0 1192 267" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M0.146484 9.04596C0.146625 1.23444 10.9146 -3.15996 16.7881 2.6983L86.5566 71.7335C100.141 68.0293 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0305 173.453 71.7364L243.212 2.71197C249.085 -3.1467 259.853 1.24702 259.854 9.05865V161.26C259.949 162.835 260 164.419 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C4.78859e-06 164.419 0.0506708 162.835 0.146484 161.26V9.04596ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.23 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.216 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z" fill="#34D399"/><path d="M819.739 240V81.5058H793.197V65.9501H863.361V81.5058H836.927V240H819.739Z" fill="#34D399"/><path d="M718.828 240L737.538 65.9501H773.11L791.711 240H774.089L769.629 190.831H740.91L736.559 240H718.828ZM742.433 175.275H768.215L764.842 138.398L760.056 81.5058H750.701L745.806 138.507L742.433 175.275Z" fill="#34D399"/><path d="M668.029 241.632C656.498 241.632 648.013 239.311 642.574 234.67C637.207 230.028 634.452 222.341 634.307 211.608C634.162 200.948 634.053 190.831 633.98 181.258C633.908 171.613 633.871 162.185 633.871 152.975C633.871 143.765 633.908 134.373 633.98 124.801C634.053 115.228 634.162 105.111 634.307 94.4508C634.452 83.9353 637.207 76.3568 642.574 71.7155C648.013 67.0016 656.498 64.6447 668.029 64.6447C679.56 64.6447 688.045 67.0016 693.484 71.7155C698.923 76.4293 701.86 84.0078 702.295 94.4508C702.44 98.2218 702.512 101.848 702.512 105.329C702.585 108.737 702.585 112.146 702.512 115.554C702.44 118.963 702.295 122.589 702.077 126.432H684.89C685.035 122.516 685.107 118.818 685.107 115.337C685.18 111.856 685.216 108.375 685.216 104.894C685.216 101.413 685.144 97.7505 684.999 93.9069C684.781 88.3953 683.403 84.6242 680.865 82.5936C678.327 80.4905 674.048 79.439 668.029 79.439C662.155 79.439 657.948 80.4905 655.41 82.5936C652.944 84.6242 651.639 88.3953 651.494 93.9069C651.276 105.293 651.095 115.772 650.95 125.345C650.805 134.845 650.733 144.091 650.733 153.084C650.733 162.004 650.805 171.25 650.95 180.823C651.095 190.323 651.276 200.803 651.494 212.261C651.639 217.772 652.981 221.58 655.519 223.683C658.057 225.786 662.227 226.837 668.029 226.837C674.411 226.837 678.907 225.786 681.518 223.683C684.201 221.58 685.651 217.772 685.869 212.261C686.014 208.925 686.086 205.661 686.086 202.47C686.086 199.207 686.05 195.581 685.978 191.592C685.905 187.531 685.796 182.745 685.651 177.233H702.948C703.238 184.195 703.383 190.36 703.383 195.726C703.455 201.02 703.383 206.314 703.165 211.608C702.73 222.341 699.757 230.028 694.245 234.67C688.806 239.311 680.067 241.632 668.029 241.632Z" fill="#34D399"/><path d="M544.569 240V65.9501H577.421C589.024 65.9501 597.364 68.1982 602.44 72.6945C607.517 77.1908 610.128 84.5879 610.273 94.8859C610.49 108.665 610.635 121.791 610.708 134.265C610.78 146.738 610.78 159.212 610.708 171.685C610.635 184.086 610.49 197.176 610.273 210.955C610.128 221.326 607.589 228.759 602.658 233.256C597.727 237.752 589.713 240 578.617 240H544.569ZM561.865 224.444H578.617C583.911 224.444 587.61 223.538 589.713 221.725C591.889 219.839 593.013 216.721 593.085 212.37C593.375 201.201 593.557 190.795 593.629 181.149C593.774 171.504 593.847 162.113 593.847 152.975C593.847 143.765 593.774 134.337 593.629 124.692C593.557 115.047 593.375 104.64 593.085 93.4717C593.013 89.1205 591.816 86.0384 589.495 84.2253C587.175 82.4123 583.15 81.5058 577.421 81.5058H561.865V224.444Z" fill="#34D399"/><path d="M452.611 240L471.322 65.9501H506.893L525.495 240H507.872L503.412 190.831H474.694L470.343 240H452.611ZM476.217 175.275H501.998L498.626 138.398L493.839 81.5058H484.484L479.589 138.507L476.217 175.275Z" fill="#34D399"/><path d="M383.097 240V65.9501H440.86V81.5058H400.393V144.708H438.684V160.263H400.393V224.444H440.86V240H383.097Z" fill="#34D399"/><path d="M292.162 240V65.9501H325.014C336.618 65.9501 344.958 68.1982 350.034 72.6945C355.111 77.1908 357.721 84.5879 357.866 94.8859C358.084 108.665 358.229 121.791 358.301 134.265C358.374 146.738 358.374 159.212 358.301 171.685C358.229 184.086 358.084 197.176 357.866 210.955C357.721 221.326 355.183 228.759 350.252 233.256C345.32 237.752 337.307 240 326.211 240H292.162ZM309.459 224.444H326.211C331.505 224.444 335.204 223.538 337.307 221.725C339.482 219.839 340.606 216.721 340.679 212.37C340.969 201.201 341.15 190.795 341.223 181.149C341.368 171.504 341.44 162.113 341.44 152.975C341.44 143.765 341.368 134.337 341.223 124.692C341.15 115.047 340.969 104.64 340.679 93.4717C340.606 89.1205 339.41 86.0384 337.089 84.2253C334.768 82.4123 330.744 81.5058 325.014 81.5058H309.459V224.444Z" fill="#34D399"/><path d="M955.193 65.9999V66.0497H1124.84V65.9999H1157.69C1169.29 66 1177.63 68.2403 1182.71 72.7402C1187.78 77.2301 1190.4 84.6298 1190.54 94.9296L1190.69 105.133C1190.82 115.224 1190.92 124.947 1190.97 134.3C1191.04 146.77 1191.04 159.25 1190.97 171.72C1190.9 184.12 1190.76 197.21 1190.54 210.99C1190.39 221.36 1187.85 228.79 1182.92 233.29C1177.99 237.78 1169.98 240.03 1158.88 240.03H921.143C910.053 240.03 902.033 237.79 897.103 233.29C892.173 228.79 889.643 221.36 889.493 210.99C889.273 197.21 889.123 184.12 889.053 171.72C888.983 159.25 888.983 146.78 889.053 134.31C889.133 121.84 889.273 108.71 889.493 94.9296C889.643 84.63 892.243 77.2204 897.323 72.7304C902.403 68.2405 910.743 65.99 922.342 65.9999H955.193Z" fill="#EF4444"/><path d="M1099.65 212V96.1899H1152.64V115.299H1121.37V143.527H1151.19V162.636H1121.37V192.891H1152.64V212H1099.65Z" fill="white"/><path d="M1035.46 212L1018.67 96.1899H1041.11L1049.65 154.24L1050.95 192.891H1057.9L1059.21 154.24L1067.75 96.1899H1090.19L1073.39 212H1035.46Z" fill="white"/><path d="M987.549 212V96.1898H1009.26V212H987.549Z" fill="white"/><path d="M928.173 212V96.1898H949.888V192.891H978.406V212H928.173Z" fill="white"/></svg></button>
          <nav class="flex shrink-0 items-baseline gap-5 whitespace-nowrap pb-0.5 text-base text-slate-400">
            <button data-action="go-home" class="${state.view === "home" || state.view === "detail" ? "font-medium text-slate-100" : "hover:text-slate-200"}">Markets</button>
            <button class="hover:text-slate-200">Live</button>
            <button class="hover:text-slate-200">Social</button>
            <button data-action="open-create-market" class="${state.view === "create" ? "font-medium text-slate-100" : "hover:text-slate-200"}">New Market</button>
          </nav>
          <div class="ml-auto flex shrink-0 items-center gap-2 pb-0.5">
            <input id="global-search" value="${state.search}" class="hidden h-9 w-[280px] rounded-full border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-300 transition focus:ring-2 lg:block xl:w-[380px]" placeholder="Trade on anything" />
            <button data-action="open-search" class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200 lg:hidden">
              <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
            </button>
            <button data-action="open-wallet" class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200">
              <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="4" width="22" height="16" rx="2" ry="2"/><line x1="1" y1="10" x2="23" y2="10"/></svg>
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
                  : `${
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
