import type { CSSProperties } from "react";

import { useAuth } from "@data/lib/auth/context";
import { openSignIn } from "./signin-store";

function shortAddress(addr: string): string {
  return `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}`;
}

export type ConnectButtonProps = {
  label?: string;
  className?: string;
};

export default function ConnectButton({ label, className }: ConnectButtonProps) {
  const auth = useAuth();

  const baseStyle: CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    gap: 8,
    padding: "10px 16px",
    borderRadius: 8,
    border: "1px solid rgba(255,255,255,0.18)",
    background: "var(--brand-cta)",
    color: "#fff",
    font: "600 14px/1 system-ui, sans-serif",
    cursor: "pointer",
  };

  if (auth.isConnected && auth.address) {
    return (
      <button
        type="button"
        className={className}
        style={{ ...baseStyle, background: "rgba(255,255,255,0.08)" }}
        onClick={auth.disconnect}
        title={`${auth.address} \u{2014} click to disconnect`}
      >
        <span
          aria-hidden
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            background: "#2ecc71",
          }}
        />
        {label ?? shortAddress(auth.address)}
      </button>
    );
  }

  return (
    <button
      type="button"
      className={className}
      style={{ ...baseStyle, opacity: auth.status === "connecting" ? 0.7 : 1 }}
      disabled={auth.status === "connecting"}
      onClick={() => openSignIn()}
    >
      {auth.status === "connecting"
        ? "Connecting\u{2026}"
        : auth.status === "expired"
          ? "Sign in again"
          : "Sign in"}
    </button>
  );
}
