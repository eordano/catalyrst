import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { EMPTY_TRANSFERS, LOADING_TRANSFER_EVENT } from "./loadingTransferProtocol";

export function getLoadingTransfers() { return typeof window === "undefined" ? EMPTY_TRANSFERS : window.__dclLoadingTransfers ?? EMPTY_TRANSFERS; }
function subscribe(listener: () => void) {
  window.addEventListener(LOADING_TRANSFER_EVENT, listener);
  return () => window.removeEventListener(LOADING_TRANSFER_EVENT, listener);
}
const empty = () => EMPTY_TRANSFERS;
const idle = () => () => {};
export function useLoadingTransfers(enabled = true) { return useSyncExternalStore(enabled ? subscribe : idle, enabled ? getLoadingTransfers : empty, empty); }

export function useTransferProgress() {
  const transfers = useLoadingTransfers();
  const baseline = useRef(transfers);
  const startedAt = useRef(Date.now());
  const samples = useRef([{ at: startedAt.current, bytes: transfers.receivedBytes }]);
  const [now, setNow] = useState(startedAt.current);
  useEffect(() => {
    const timer = window.setInterval(() => {
      const at = Date.now();
      samples.current.push({ at, bytes: getLoadingTransfers().receivedBytes });
      samples.current = samples.current.filter(sample => at - sample.at <= 5000);
      setNow(at);
    }, 500);
    return () => window.clearInterval(timer);
  }, []);
  const first = samples.current[0] ?? { at: now, bytes: transfers.receivedBytes };
  const duration = (now - first.at) / 1000;
  const speed = duration >= 1.5 ? Math.max(0, transfers.receivedBytes - first.bytes) / duration : 0;
  const received = Math.max(0, transfers.receivedBytes - baseline.current.receivedBytes);
  const completed = Math.max(0, transfers.completed - baseline.current.completed);
  const failed = Math.max(0, transfers.failed - baseline.current.failed);
  const known = transfers.unknownActive === 0;
  const total = received + transfers.remainingBytes;
  const estimate = known && speed > 0 && transfers.remainingBytes > 0 ? Math.ceil(transfers.remainingBytes / speed) : null;
  return { ...transfers, received, completed, failed, speed, estimate, total, known, elapsed: Math.floor((now - startedAt.current) / 1000) };
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
export function formatWait(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}
