import { useEffect, useRef, useState } from "react";
import { truncateAddress } from "../../data/format";
import Icon from "../frames/SidebarDesignIcon";
import "./walletaddress.css";

export default function WalletAddress({ address, compact = false }: { address: string; compact?: boolean }) {
  const [status, setStatus] = useState("");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const copy = async () => {
    clearTimeout(timer.current);
    try {
      await navigator.clipboard.writeText(address);
      setStatus("Copied");
      timer.current = setTimeout(() => setStatus(""), 1000);
    } catch { setStatus("Could not copy"); }
  };
  return <div className="wallet-address">
    <button type="button" aria-label="Copy wallet address" title={address} onClick={() => void copy()}>
      <span>{compact ? truncateAddress(address) : address}</span><Icon name="copy" />
    </button>
    <span className="wallet-address__status" role="status">{status}</span>
  </div>;
}
