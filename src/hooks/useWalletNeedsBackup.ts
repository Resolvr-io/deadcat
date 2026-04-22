import { useEffect, useState } from "react";

// localStorage-backed flag that persists across app restarts. A custom
// window event lets other components in the same tab react to changes
// without needing to poll localStorage. Pattern ported from Astrolabe.

const KEY = "deadcat:walletNeedsBackup";
const EVENT = "deadcat:walletNeedsBackupChanged";

function read(): boolean {
  try {
    return localStorage.getItem(KEY) === "true";
  } catch {
    return false;
  }
}

export function setWalletNeedsBackup(value: boolean): void {
  try {
    if (value) localStorage.setItem(KEY, "true");
    else localStorage.removeItem(KEY);
  } catch {
    // localStorage unavailable — silently ignore
  }
  window.dispatchEvent(new CustomEvent(EVENT));
}

/** React to the backup-needed flag from any component. */
export function useWalletNeedsBackup(): boolean {
  const [flag, setFlag] = useState<boolean>(read);
  useEffect(() => {
    const handler = () => setFlag(read());
    window.addEventListener("storage", handler);
    window.addEventListener(EVENT, handler);
    return () => {
      window.removeEventListener("storage", handler);
      window.removeEventListener(EVENT, handler);
    };
  }, []);
  return flag;
}
