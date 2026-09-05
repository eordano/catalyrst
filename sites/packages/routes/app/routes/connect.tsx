import { useEffect, useState, type CSSProperties } from "react";

import ConnectButton from "@features/components/auth/ConnectButton";
import { catalystBase } from "@data/lib/catalyst/client";
import { useAuth } from "@data/lib/auth/context";
import type { SignedRequest } from "@data/lib/auth/signer";

import type { Route } from "./+types/connect";

export function meta(_args: Route.MetaArgs) {
  return [
    { title: "Wallet Connect \u{2014} Decentraland" },
    {
      name: "description",
      content: "Sign in and inspect the ADR-44 signed-fetch identity.",
    },
  ];
}

const TEST_METHOD = "GET";
const TEST_PATH = "/world-storage/usage/world";

const mono: CSSProperties = {
  font: "12px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace",
  whiteSpace: "pre-wrap",
  wordBreak: "break-all",
  background: "#0d0d12",
  color: "#d6d6e0",
  border: "1px solid rgba(255,255,255,0.08)",
  borderRadius: 8,
  padding: 12,
  margin: 0,
};

const card: CSSProperties = {
  border: "1px solid rgba(255,255,255,0.1)",
  borderRadius: 12,
  padding: 20,
  marginTop: 16,
  background: "rgba(255,255,255,0.02)",
};

export default function ConnectRoute() {
  const auth = useAuth();
  const [signed, setSigned] = useState<SignedRequest | null>(null);
  const [signError, setSignError] = useState<string | null>(null);
  const [liveResult, setLiveResult] = useState<string | null>(null);
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  async function onSignTest() {
    setSignError(null);
    setLiveResult(null);
    try {
      const result = await auth.sign(TEST_METHOD, TEST_PATH, {});
      setSigned(result);
    } catch (err) {
      setSignError((err as Error)?.message ?? "Failed to sign.");
    }
  }

  async function onSendLive() {
    setLiveResult("Sending\u{2026}");
    try {
      const url = `${catalystBase()}${TEST_PATH}`;
      const res = await auth.fetch(url, { method: TEST_METHOD });
      const text = await res.text();
      setLiveResult(`HTTP ${res.status} ${res.statusText}\n\n${text.slice(0, 800)}`);
    } catch (err) {
      setLiveResult(`Request failed: ${(err as Error)?.message ?? "error"}`);
    }
  }

  return (
    <main
      style={{
        maxWidth: 760,
        margin: "0 auto",
        padding: "40px 24px 80px",
        color: "#e8e8f0",
        font: "15px/1.6 system-ui, sans-serif",
      }}
    >
      <h1 style={{ fontSize: 28, marginBottom: 4 }}>Wallet Connect</h1>
      <p style={{ color: "rgba(232,232,240,0.65)", marginTop: 0 }}>
        Demo of the auth module powering catalyrst signed-fetch (ADR-44). Connect a
        browser wallet, then sign a test request payload to see the auth chain
        that write surfaces send.
      </p>

      <div style={{ marginTop: 20 }}>
        <ConnectButton />
        {mounted && !auth.walletAvailable && (
          <p style={{ color: "#ffb454", marginTop: 12 }}>
            No browser wallet detected. Install MetaMask (or another EIP-1193
            wallet) to try this.
          </p>
        )}
        {auth.error && (
          <p style={{ color: "#ff6b6b", marginTop: 12 }}>{auth.error}</p>
        )}
      </div>

      {auth.isConnected && auth.identity && (
        <>
          <section style={card}>
            <h2 style={{ fontSize: 18, marginTop: 0 }}>Signed identity</h2>
            <dl style={{ margin: 0 }}>
              <Row label="Wallet (signer)" value={auth.address ?? ""} />
              <Row label="Ephemeral address" value={auth.identity.ephemeral.address} />
              <Row label="Expires" value={auth.identity.expiration} />
            </dl>
            <p style={{ marginBottom: 6, marginTop: 16, fontWeight: 600 }}>
              Identity auth chain ([SIGNER, ECDSA_EPHEMERAL])
            </p>
            <pre style={mono}>{JSON.stringify(auth.identity.authChain, null, 2)}</pre>
          </section>

          <section style={card}>
            <h2 style={{ fontSize: 18, marginTop: 0 }}>Sign a test payload</h2>
            <p style={{ marginTop: 0, color: "rgba(232,232,240,0.65)" }}>
              <code>
                {TEST_METHOD} {TEST_PATH}
              </code>{" "}
              &#x2014; signed locally with the ephemeral key (no wallet prompt).
            </p>
            <button type="button" style={btn} onClick={onSignTest}>
              Sign test request
            </button>
            {signError && (
              <p style={{ color: "#ff6b6b" }}>{signError}</p>
            )}
            {signed && (
              <>
                <p style={{ marginBottom: 6, marginTop: 16, fontWeight: 600 }}>
                  Signed payload string
                </p>
                <pre style={mono}>{signed.payload}</pre>
                <p style={{ marginBottom: 6, marginTop: 16, fontWeight: 600 }}>
                  Outgoing headers
                </p>
                <pre style={mono}>
                  {Object.entries(signed.headers)
                    .map(([k, v]) => `${k}: ${v}`)
                    .join("\n")}
                </pre>

                <button
                  type="button"
                  style={{ ...btn, marginTop: 16 }}
                  onClick={onSendLive}
                >
                  Send it live (optional)
                </button>
                <p style={{ fontSize: 13, color: "rgba(232,232,240,0.5)" }}>
                  Fires a real signed request to{" "}
                  <code>{catalystBase()}{TEST_PATH}</code>. A valid signature
                  returns this wallet&apos;s per-world byte usage; an unsigned or
                  bad chain is rejected with HTTP 401 (the backend enforces
                  ADR-44).
                </p>
                {liveResult && <pre style={mono}>{liveResult}</pre>}
              </>
            )}
          </section>
        </>
      )}
    </main>
  );
}

const btn: CSSProperties = {
  padding: "10px 16px",
  borderRadius: 8,
  border: "1px solid rgba(255,255,255,0.18)",
  background: "rgba(255,255,255,0.08)",
  color: "#fff",
  font: "600 14px/1 system-ui, sans-serif",
  cursor: "pointer",
};

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: "flex", gap: 12, padding: "4px 0" }}>
      <dt style={{ width: 160, color: "rgba(232,232,240,0.55)" }}>{label}</dt>
      <dd
        style={{
          margin: 0,
          font: "13px/1.5 ui-monospace, monospace",
          wordBreak: "break-all",
        }}
      >
        {value}
      </dd>
    </div>
  );
}
