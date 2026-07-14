import { useEffect, useMemo, useState, type CSSProperties } from "react";

import { openSignIn } from "@features/components/auth/signin-store";
import { useAuth } from "@data/lib/auth/context";
import { isDevHost } from "@data/lib/auth/dev-host";
import { buildEphemeralMessage } from "@data/lib/auth/identity";
import { isDesktopShell, RELAY_PATH } from "@data/lib/auth/native-shell";
import { getThirdwebSession } from "@data/lib/auth/thirdweb/session";
import { makeInAppSigner } from "@data/lib/auth/thirdweb/signer";
import { hasWallet, personalSign } from "@data/lib/auth/wallet";
import { shortAddress } from "@data/lib/catalyst/format/address";

import type { Route } from "./+types/auth.native";

export function meta(_args: Route.MetaArgs) {
  return [{ title: "Authorize desktop app \u{2014} Decentraland" }];
}

const ADDR_RE = /^0x[0-9a-fA-F]{40}$/;
const MAX_EXPIRATION_MS = 31 * 24 * 60 * 60 * 1000;

type Params = { id: string; ephemeral: string; expiration: string };

function readParams(search: string): Params | null {
  const p = new URLSearchParams(search);
  const id = p.get("id") ?? "";
  const ephemeral = p.get("ephemeral") ?? "";
  const expiration = p.get("expiration") ?? "";
  if (!/^[0-9a-fA-F-]{30,80}$/.test(id)) return null;
  if (!ADDR_RE.test(ephemeral)) return null;
  const exp = Date.parse(expiration);
  if (
    !Number.isFinite(exp) ||
    exp <= Date.now() ||
    exp > Date.now() + MAX_EXPIRATION_MS
  ) {
    return null;
  }
  return { id, ephemeral, expiration };
}

async function signWithBrowserSession(
  signer: string,
  message: string,
): Promise<string> {
  const tw = getThirdwebSession();
  if (tw && tw.address.toLowerCase() === signer.toLowerCase()) {
    return makeInAppSigner({ token: tw.token, walletAddress: tw.address })
      .personalSign(message);
  }
  if (isDevHost()) {
    try {
      const pk = window.localStorage.getItem("dcl:auth:dev-signer-pk:v1");
      if (pk?.startsWith("0x")) {
        const { privateKeyToAccount } = await import("viem/accounts");
        const account = privateKeyToAccount(pk as `0x${string}`);
        if (account.address.toLowerCase() === signer.toLowerCase()) {
          return account.signMessage({ message });
        }
      }
    } catch {
    }
  }
  if (hasWallet()) return personalSign(message, signer);
  throw new Error(
    "This browser session cannot sign: no wallet is available and the " +
      "signed-in account has no enclave session. Sign in here first with " +
      "the account you want to use.",
  );
}

const card: CSSProperties = {
  maxWidth: 420,
  margin: "12vh auto 0",
  padding: "32px 28px",
  borderRadius: 12,
  background: "#1b1822",
  color: "#fcfcfc",
  fontFamily: "Inter, system-ui, sans-serif",
  textAlign: "center",
};
const chip: CSSProperties = {
  display: "inline-block",
  padding: "4px 10px",
  borderRadius: 8,
  background: "#2c2837",
  fontFamily: "monospace",
  fontSize: 14,
  margin: "8px 0 16px",
};
const button: CSSProperties = {
  display: "inline-block",
  padding: "10px 22px",
  borderRadius: 8,
  border: "none",
  background: "var(--brand-cta)",
  color: "#fff",
  fontSize: 15,
  fontWeight: 600,
  cursor: "pointer",
};
const subtle: CSSProperties = { color: "#a09ba8", fontSize: 14, lineHeight: 1.5 };

type Step = "invalid" | "consent" | "signing" | "done" | "error";

export default function AuthNativeRoute() {
  const auth = useAuth();
  const [params, setParams] = useState<Params | null | undefined>(undefined);
  const [step, setStep] = useState<Step>("consent");
  const [error, setError] = useState<string | null>(null);
  const [inShell, setInShell] = useState(false);

  useEffect(() => {
    setParams(readParams(window.location.search));
    setInShell(isDesktopShell());
  }, []);

  const shortAddr = useMemo(
    () => (auth.address ? shortAddress(auth.address) : ""),
    [auth.address],
  );

  async function onApprove() {
    if (!params || !auth.address || step === "signing") return;
    setStep("signing");
    setError(null);
    try {
      const message = buildEphemeralMessage(
        params.ephemeral.toLowerCase(),
        new Date(params.expiration),
      );
      const signature = await signWithBrowserSession(auth.address, message);
      const res = await fetch(RELAY_PATH, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          id: params.id,
          signer: auth.address,
          signature,
          ephemeral: params.ephemeral.toLowerCase(),
          expiration: params.expiration,
        }),
      });
      if (!res.ok) throw new Error(`relay rejected the approval (${res.status})`);
      setStep("done");
    } catch (err) {
      setError((err as Error)?.message ?? "Failed to authorize.");
      setStep("error");
    }
  }

  if (params === undefined) return null;

  if (params === null) {
    return (
      <div style={card}>
        <h2>Invalid request</h2>
        <p style={subtle}>
          This authorization link is malformed or expired. Start the sign-in
          again from the desktop app.
        </p>
      </div>
    );
  }

  if (inShell) {
    return (
      <div style={card}>
        <h2>Open in your browser</h2>
        <p style={subtle}>
          This page authorizes the desktop app and is meant to open in your
          regular browser, where you are already signed in. (The shell did not
          intercept this navigation &#x2014; copy the URL into your browser.)
        </p>
      </div>
    );
  }

  if (!auth.isConnected) {
    return (
      <div style={card}>
        <h2>Authorize the desktop Creator Hub</h2>
        <p style={subtle}>
          Sign in here first &#x2014; the desktop app will use this browser session,
          so you only ever sign in once.
        </p>
        <button style={button} onClick={() => openSignIn()}>
          Sign in
        </button>
      </div>
    );
  }

  if (step === "done") {
    return (
      <div style={card}>
        <h2>You're signed in on the desktop app</h2>
        <p style={subtle}>You can close this tab and return to the Creator Hub.</p>
      </div>
    );
  }

  return (
    <div style={card}>
      <h2>Authorize the desktop Creator Hub</h2>
      <p style={subtle}>The desktop app on this machine wants to sign in as</p>
      <span style={chip}>{shortAddr}</span>
      <p style={subtle}>
        It gets its own session key, valid until{" "}
        {new Date(params.expiration).toLocaleString()}. Your wallet key never
        leaves this browser.
        {hasWallet() && !getThirdwebSession()
          ? " Your wallet will ask for one signature."
          : ""}
      </p>
      {error && (
        <p style={{ ...subtle, color: "#ff5c77" }} role="alert">
          {error}
        </p>
      )}
      <button style={button} onClick={onApprove} disabled={step === "signing"}>
        {step === "signing" ? "Waiting for signature\u{2026}" : "Authorize"}
      </button>
    </div>
  );
}
