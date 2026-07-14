import { useEffect, useState, type CSSProperties } from "react";
import { data, useLoaderData } from "react-router";

import { buildEphemeralMessage } from "@data/lib/auth/identity";
import { PAIR_API_PATH } from "@data/lib/auth/pair";
import { pairStore } from "@data/lib/auth/pair-store.server";
import { connectWallet, hasWallet, personalSign } from "@data/lib/auth/wallet";

import type { Route } from "./+types/auth.pair.$session";

export function meta(_args: Route.MetaArgs) {
  return [{ title: "Sign in with your phone \u{2014} Decentraland" }];
}

const ID_RE = /^[A-Za-z0-9_-]{20,64}$/;
const NO_STORE = { headers: { "cache-control": "no-store" } };

export async function loader({ request, params }: Route.LoaderArgs) {
  const host = new URL(request.url).host;
  const id = params.session ?? "";
  if (!ID_RE.test(id)) return data({ state: "missing" as const, host }, NO_STORE);
  const session = pairStore.get(id);
  if (!session) return data({ state: "missing" as const, host }, NO_STORE);
  if (Date.now() > session.expiresAt) {
    return data({ state: "expired" as const, host }, NO_STORE);
  }
  if (session.completed) {
    return data({ state: "completed" as const, host }, NO_STORE);
  }
  return data(
    {
      state: "pending" as const,
      host,
      id,
      ephemeral: session.ephemeral,
      expiration: session.expiration,
    },
    NO_STORE,
  );
}

const card: CSSProperties = {
  maxWidth: 440,
  margin: "6vh auto 0",
  padding: "28px 22px 32px",
  borderRadius: 12,
  background: "#1b1822",
  color: "#fcfcfc",
  fontFamily: "Inter, system-ui, sans-serif",
  textAlign: "center",
};
const subtle: CSSProperties = { color: "#a09ba8", fontSize: 15, lineHeight: 1.5 };
const messageBox: CSSProperties = {
  margin: "16px 0",
  padding: "12px 14px",
  borderRadius: 8,
  background: "#2c2837",
  fontFamily: "ui-monospace, monospace",
  fontSize: 13,
  lineHeight: 1.6,
  textAlign: "left",
  whiteSpace: "pre-wrap",
  wordBreak: "break-word",
};
const button: CSSProperties = {
  display: "block",
  width: "100%",
  padding: "14px 22px",
  borderRadius: 10,
  border: "none",
  background: "var(--brand-cta)",
  color: "#fff",
  fontSize: 16,
  fontWeight: 600,
  cursor: "pointer",
};
const walletLink: CSSProperties = {
  ...button,
  background: "#2c2837",
  textDecoration: "none",
  boxSizing: "border-box",
  marginTop: 10,
};
const brandFooter: CSSProperties = {
  ...subtle,
  fontSize: 12,
  marginTop: 22,
  paddingTop: 14,
  borderTop: "1px solid #2c2837",
};

function walletDeepLinks(pageUrl: string): { name: string; href: string }[] {
  const encoded = encodeURIComponent(pageUrl);
  const schemeless = pageUrl.replace(/^https?:\/\//, "");
  let origin = "";
  try {
    origin = new URL(pageUrl).origin;
  } catch {
  }
  return [
    { name: "MetaMask", href: `https://metamask.app.link/dapp/${schemeless}` },
    { name: "Coinbase Wallet", href: `https://go.cb-w.com/dapp?cb_url=${encoded}` },
    {
      name: "Trust Wallet",
      href: `https://link.trustwallet.com/open_url?coin_id=60&url=${encoded}`,
    },
    {
      name: "Phantom",
      href: `https://phantom.app/ul/browse/${encoded}?ref=${encodeURIComponent(origin)}`,
    },
    {
      name: "OKX Wallet",
      href: `https://web3.okx.com/download?deeplink=${encodeURIComponent(
        `okx://wallet/dapp/url?dappUrl=${encoded}`,
      )}`,
    },
  ];
}

type Step = "consent" | "signing" | "done" | "error";

export default function AuthPairRoute() {
  const loaded = useLoaderData<typeof loader>();
  const [step, setStep] = useState<Step>("consent");
  const [error, setError] = useState<string | null>(null);
  const [walletReady, setWalletReady] = useState<boolean | null>(null);
  const [pageUrl, setPageUrl] = useState("");

  useEffect(() => {
    setPageUrl(window.location.href);
    setWalletReady(hasWallet() || null);
    let tries = 0;
    const poll = setInterval(() => {
      tries += 1;
      if (hasWallet()) {
        setWalletReady(true);
        clearInterval(poll);
      } else if (tries >= 7) {
        setWalletReady(false);
        clearInterval(poll);
      }
    }, 450);
    const onInit = () => {
      setWalletReady(true);
      clearInterval(poll);
    };
    window.addEventListener("ethereum#initialized", onInit, { once: true });
    return () => {
      clearInterval(poll);
      window.removeEventListener("ethereum#initialized", onInit);
    };
  }, []);

  if (loaded.state === "missing" || loaded.state === "expired") {
    return (
      <div style={card}>
        <h2>This sign-in link {loaded.state === "expired" ? "expired" : "isn't valid"}</h2>
        <p style={subtle}>
          Codes only last a few minutes. Go back to <strong>{loaded.host}</strong> on
          your computer and scan a fresh QR code.
        </p>
      </div>
    );
  }

  if (loaded.state === "completed") {
    return (
      <div style={card}>
        <h2>This code was already used</h2>
        <p style={subtle}>
          Each QR code signs you in once. If that wasn't you, start a new
          sign-in on your computer.
        </p>
      </div>
    );
  }

  const { id, ephemeral, expiration } = loaded;
  const message = buildEphemeralMessage(ephemeral, new Date(expiration));

  async function onApprove() {
    if (step === "signing") return;
    setStep("signing");
    setError(null);
    try {
      const signer = await connectWallet();
      const signature = await personalSign(message, signer);
      const res = await fetch(PAIR_API_PATH, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ kind: "complete", id, signer, signature }),
      });
      if (res.status === 409) throw new Error("This code was already used.");
      if (res.status === 410 || res.status === 404) {
        throw new Error("This code expired \u{2014} scan a fresh one.");
      }
      if (!res.ok) {
        const body = (await res.json().catch(() => null)) as { error?: string } | null;
        throw new Error(body?.error ?? `The sign-in couldn't be completed (${res.status}).`);
      }
      setStep("done");
    } catch (err) {
      setError((err as Error)?.message ?? "The wallet didn't sign.");
      setStep("error");
    }
  }

  if (step === "done") {
    return (
      <div style={card}>
        <h2>You're signed in on your computer</h2>
        <p style={subtle}>
          Head back to <strong>{loaded.host}</strong> {"\u{2014}"} it picks up the sign-in
          within a couple of seconds. You can close this tab.
        </p>
      </div>
    );
  }

  return (
    <div style={card}>
      <h2>Sign in to {loaded.host}</h2>
      <p style={subtle}>
        Your computer is asking to sign in to Decentraland with the wallet on
        this phone. Approving signs the one message below, which authorizes a
        temporary session key on that computer until{" "}
        {new Date(loaded.expiration).toLocaleString()}. Your wallet key never
        leaves your wallet, and no transaction is sent.
      </p>
      <pre style={messageBox}>{message}</pre>
      {error && (
        <p style={{ ...subtle, color: "#ff5c77", marginBottom: 12 }} role="alert">
          {error}
        </p>
      )}
      {walletReady ? (
        <button style={button} onClick={onApprove} disabled={step === "signing"}>
          {step === "signing" ? "Waiting for your wallet\u{2026}" : "Approve in your wallet"}
        </button>
      ) : walletReady === false ? (
        <>
          <p style={subtle}>
            No wallet found in this browser. Open this page inside your wallet
            app instead {"\u{2014}"} signing happens in the wallet's built-in browser:
          </p>
          {walletDeepLinks(pageUrl).map((w) => (
            <a key={w.name} style={walletLink} href={w.href}>
              Open in {w.name}
            </a>
          ))}
          <p style={{ ...subtle, fontSize: 13, marginTop: 16 }}>
            Using another wallet app? Open its built-in browser and go to this
            page's address. This is Decentraland's own pairing page {"\u{2014}"} it works
            with any Ethereum wallet that has a dapp browser, without
            WalletConnect.
          </p>
        </>
      ) : null}
      <p style={brandFooter}>
        Secured by <strong>LibreConnect</strong> {"\u{2014}"} an open, self-hosted pairing
        standard. No WalletConnect, no project ID.
      </p>
    </div>
  );
}
