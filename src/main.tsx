import { init as initBitcoinConnect } from "@getalby/bitcoin-connect-react";
import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { queryClient } from "./queries/queryClient";
import "./style.css";

// Singleton init. `filters: ['nwc']` drops LNC / LNbits / browser
// extension paths — the only connect flow we support today is NWC.
// `showBalance: false` skips BC's own balance rendering; if we ever
// want a balance surface we'll build it to match our palette.
initBitcoinConnect({
  appName: "Deadcat",
  showBalance: false,
  filters: ["nwc"],
});

const rootElement = document.getElementById("app");
if (!rootElement) throw new Error("Root element not found");
createRoot(rootElement).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
