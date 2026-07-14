import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { data, useLoaderData } from "react-router";

import {
  connectWallet,
  getConnectedAddress,
  hasWallet,
  selectWallet,
  walletProvider,
} from "@data/lib/auth/wallet";

import type { Route } from "./+types/auth.requests.$id";

export function meta(_args: Route.MetaArgs) {
  return [{ title: "Approve sign-in request \u{2014} Decentraland" }];
}

const AUTH_API = "/auth-api";
const ID_RE = /^[0-9a-fA-F-]{30,80}$/;
const NO_STORE = { headers: { "cache-control": "no-store" } };

export async function loader({ request, params }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const id = params.id ?? "";
  return data(
    {
      id,
      valid: ID_RE.test(id),
      host: url.host,
      loginMethod: url.searchParams.get("loginMethod") ?? "",
      isDeepLink: (url.searchParams.get("flow") ?? "").toLowerCase() === "deeplink",
    },
    NO_STORE,
  );
}

type RecoverResponse = {
  expiration: string;
  code: number;
  method: string;
  params: unknown[];
  sender?: string;
  challenge: string;
};

type LoadResult =
  | { kind: "ok"; request: RecoverResponse }
  | { kind: "not_found" }
  | { kind: "expired" }
  | { kind: "fulfilled" }
  | { kind: "error"; message: string };

async function loadRequest(id: string): Promise<LoadResult> {
  let res: Response;
  try {
    res = await fetch(`${AUTH_API}/v2/requests/${encodeURIComponent(id)}`, {
      headers: { accept: "application/json" },
      cache: "no-store",
    });
  } catch {
    return { kind: "error", message: "Couldn't reach the sign-in server." };
  }
  if (res.ok) {
    return { kind: "ok", request: (await res.json()) as RecoverResponse };
  }
  const body = (await res.json().catch(() => null)) as { error?: string } | null;
  const err = body?.error ?? "";
  if (/already been fulfilled|already has a response/.test(err)) return { kind: "fulfilled" };
  if (/has expired/.test(err)) return { kind: "expired" };
  if (/not found/.test(err)) return { kind: "not_found" };
  return { kind: "error", message: err || `The request couldn't be loaded (${res.status}).` };
}

async function requiresValidation(id: string): Promise<boolean> {
  try {
    const res = await fetch(`${AUTH_API}/v2/requests/${encodeURIComponent(id)}/validation`, {
      cache: "no-store",
    });
    if (!res.ok) return false;
    const body = (await res.json()) as { requiresValidation?: boolean };
    return body?.requiresValidation === true;
  } catch {
    return false;
  }
}

type OutcomeError = { code: number; message: string };

async function postOutcome(
  id: string,
  body: { sender: string; result?: unknown; error?: OutcomeError },
): Promise<Response> {
  return fetch(`${AUTH_API}/v2/requests/${encodeURIComponent(id)}/outcome`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

// Mirrors catalyrst-explorer-api auth_api/validation.rs: even a request that slipped past the
// server guard must never let the wallet sign a Decentraland ephemeral-identity payload (which
// would mint an auth chain that impersonates the user) or use the retired sign-in method.
const EPHEMERAL_ADDRESS_PREFIX = "Ephemeral address: ";
const EXPIRATION_PREFIX = "Expiration: ";

function decodeHexMessage(value: string): string | null {
  const body = value.startsWith("0x") || value.startsWith("0X") ? value.slice(2) : null;
  if (body === null || body.length % 2 !== 0) return null;
  const bytes = new Uint8Array(body.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    const byte = Number.parseInt(body.slice(i * 2, i * 2 + 2), 16);
    if (Number.isNaN(byte)) return null;
    bytes[i] = byte;
  }
  return new TextDecoder().decode(bytes);
}

function isEphemeralText(value: string): boolean {
  const lines = value.replace(/\r/g, "").split("\n").slice(1);
  const addressLine = lines[0];
  const expirationLine = lines[1];
  if (addressLine === undefined || expirationLine === undefined) return false;
  if (!addressLine.startsWith(EPHEMERAL_ADDRESS_PREFIX)) return false;
  if (!expirationLine.startsWith(EXPIRATION_PREFIX)) return false;
  return expirationLine.slice(EXPIRATION_PREFIX.length).trim().length > 0;
}

function isEphemeralMessage(value: string): boolean {
  if (isEphemeralText(value)) return true;
  const decoded = decodeHexMessage(value);
  return decoded !== null && isEphemeralText(decoded);
}

function unsupportedReason(method: string, params: unknown[]): string | null {
  if (method.trim().toLowerCase() === "dcl_personal_sign") {
    return "This request uses a retired sign-in method. Update the app that opened it.";
  }
  if (params.some((p) => typeof p === "string" && isEphemeralMessage(p))) {
    return "This request tried to sign a Decentraland identity payload and was blocked.";
  }
  return null;
}

const RDNS_BY_METHOD: Record<string, string> = {
  metamask: "io.metamask",
  coinbase: "com.coinbase.wallet",
  rabby: "io.rabby",
};

function decodeSignatureMessage(value: unknown): string | null {
  if (typeof value !== "string") return null;
  if (value.startsWith("0x") || value.startsWith("0X")) {
    const decoded = decodeHexMessage(value);
    if (decoded !== null) return decoded;
  }
  return value;
}

function describeRequest(request: RecoverResponse): { title: string; detail: string } {
  const method = request.method;
  if (method === "personal_sign") {
    const message = decodeSignatureMessage(request.params[0]);
    return {
      title: "Sign a message",
      detail: message ?? JSON.stringify(request.params, null, 2),
    };
  }
  if (method === "eth_signTypedData_v4" || method === "eth_signTypedData") {
    const message = decodeSignatureMessage(request.params[1]);
    return {
      title: "Sign typed data",
      detail: message ?? JSON.stringify(request.params, null, 2),
    };
  }
  if (method === "eth_sendTransaction") {
    return { title: "Send a transaction", detail: JSON.stringify(request.params[0], null, 2) };
  }
  return { title: method, detail: JSON.stringify(request.params, null, 2) };
}

function formatCountdown(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

type Phase =
  | "loading"
  | "invalid"
  | "not_found"
  | "expired"
  | "fulfilled"
  | "load_error"
  | "unsupported"
  | "ready"
  | "different_account"
  | "signing"
  | "done"
  | "denied"
  | "error";

const card: CSSProperties = {
  maxWidth: 460,
  margin: "6vh auto 0",
  padding: "28px 24px 32px",
  borderRadius: 12,
  background: "#1b1822",
  color: "#fcfcfc",
  fontFamily: "Inter, system-ui, sans-serif",
  textAlign: "center",
};
const subtle: CSSProperties = { color: "#a09ba8", fontSize: 15, lineHeight: 1.5 };
const codeBox: CSSProperties = {
  margin: "20px auto",
  padding: "16px 20px",
  borderRadius: 12,
  background: "#2c2837",
  display: "inline-block",
};
const codeDigits: CSSProperties = {
  fontFamily: "ui-monospace, monospace",
  fontSize: 52,
  fontWeight: 700,
  letterSpacing: 6,
  lineHeight: 1,
  color: "#fff",
};
const codeLabel: CSSProperties = { ...subtle, fontSize: 13, marginTop: 8, textTransform: "uppercase", letterSpacing: 1 };
const detailBox: CSSProperties = {
  margin: "16px 0",
  padding: "12px 14px",
  borderRadius: 8,
  background: "#242030",
  fontFamily: "ui-monospace, monospace",
  fontSize: 13,
  lineHeight: 1.6,
  textAlign: "left",
  whiteSpace: "pre-wrap",
  wordBreak: "break-word",
  maxHeight: 220,
  overflow: "auto",
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
  marginTop: 12,
};
const denyButton: CSSProperties = {
  ...button,
  background: "transparent",
  color: "#a09ba8",
  fontWeight: 500,
  marginTop: 8,
};
const chip: CSSProperties = {
  display: "inline-block",
  padding: "4px 10px",
  borderRadius: 8,
  background: "#2c2837",
  fontFamily: "monospace",
  fontSize: 13,
  margin: "6px 0",
};
const ackRow: CSSProperties = {
  ...subtle,
  display: "flex",
  gap: 8,
  textAlign: "left",
  alignItems: "flex-start",
  margin: "14px 2px 4px",
  fontSize: 14,
};
const brandFooter: CSSProperties = {
  ...subtle,
  fontSize: 12,
  marginTop: 22,
  paddingTop: 14,
  borderTop: "1px solid #2c2837",
};

export default function AuthRequestRoute() {
  const loaded = useLoaderData<typeof loader>();
  const [phase, setPhase] = useState<Phase>(loaded.valid ? "loading" : "invalid");
  const [request, setRequest] = useState<RecoverResponse | null>(null);
  const [mustValidate, setMustValidate] = useState(false);
  const [acknowledged, setAcknowledged] = useState(false);
  const [remaining, setRemaining] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [walletName, setWalletName] = useState<string | null>(null);
  const startedRef = useRef(false);

  useEffect(() => {
    if (!loaded.valid || startedRef.current) return;
    startedRef.current = true;
    let cancelled = false;

    void (async () => {
      const [result, needsValidation] = await Promise.all([
        loadRequest(loaded.id),
        requiresValidation(loaded.id),
      ]);
      if (cancelled) return;

      if (result.kind !== "ok") {
        setPhase(
          result.kind === "not_found"
            ? "not_found"
            : result.kind === "expired"
              ? "expired"
              : result.kind === "fulfilled"
                ? "fulfilled"
                : "load_error",
        );
        if (result.kind === "error") setError(result.message);
        return;
      }

      const req = result.request;
      const blocked = unsupportedReason(req.method, req.params ?? []);
      if (blocked) {
        setError(blocked);
        setPhase("unsupported");
        return;
      }

      const expiresAt = Date.parse(req.expiration);
      if (Number.isFinite(expiresAt) && expiresAt <= Date.now()) {
        setPhase("expired");
        return;
      }

      setRequest(req);
      setMustValidate(needsValidation);
      setPhase("ready");
    })();

    return () => {
      cancelled = true;
    };
  }, [loaded.id, loaded.valid]);

  useEffect(() => {
    if (!request) return;
    const expiresAt = Date.parse(request.expiration);
    if (!Number.isFinite(expiresAt)) return;
    const tick = () => {
      const left = expiresAt - Date.now();
      setRemaining(left);
      if (left <= 0) setPhase((prev) => (prev === "ready" ? "expired" : prev));
    };
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [request]);

  const onApprove = useCallback(async () => {
    if (!request || phase === "signing") return;
    if (mustValidate && !acknowledged) return;
    setError(null);
    setPhase("signing");

    let sender: string;
    try {
      const rdns = RDNS_BY_METHOD[loaded.loginMethod.toLowerCase()] ?? null;
      selectWallet(rdns);
      sender = (await getConnectedAddress()) ?? (await connectWallet());
    } catch (err) {
      setError((err as Error)?.message ?? "Couldn't connect your wallet.");
      setPhase("ready");
      return;
    }

    if (request.sender && request.sender.toLowerCase() !== sender.toLowerCase()) {
      setPhase("different_account");
      return;
    }

    if (mustValidate && !(await requiresValidation(loaded.id))) {
      setMustValidate(false);
    }

    let executed = false;
    try {
      const result = await walletProvider().request({
        method: request.method,
        params: (request.params ?? []) as unknown[],
      });
      executed = true;
      const res = await postOutcome(loaded.id, { sender, result });
      if (!res.ok) {
        const body = (await res.json().catch(() => null)) as { error?: string } | null;
        const message = body?.error ?? "";
        if (!/already been fulfilled|already has a response/.test(message)) {
          throw new Error(message || `The approval couldn't be recorded (${res.status}).`);
        }
      }
      setPhase("done");
    } catch (err) {
      if (executed) {
        setPhase("done");
        return;
      }
      if (isUserRejection(err)) {
        await postOutcome(loaded.id, {
          sender,
          error: { code: -32003, message: "Request rejected" },
        }).catch(() => null);
        setPhase("denied");
        return;
      }
      const message = (err as Error)?.message ?? "The wallet couldn't complete the request.";
      await postOutcome(loaded.id, { sender, error: { code: 999, message } }).catch(() => null);
      setError(message);
      setPhase("error");
    }
  }, [request, phase, mustValidate, acknowledged, loaded.id, loaded.loginMethod]);

  const onDeny = useCallback(async () => {
    if (!request) return;
    setPhase("denied");
    const sender = (await getConnectedAddress().catch(() => null)) ?? request.sender ?? "";
    if (sender) {
      await postOutcome(loaded.id, {
        sender,
        error: { code: -32003, message: "Request rejected" },
      }).catch(() => null);
    }
  }, [request, loaded.id]);

  useEffect(() => {
    setWalletName(hasWallet() ? "wallet" : null);
  }, []);

  if (phase === "loading") {
    return (
      <div style={card}>
        <p style={subtle}>Loading the sign-in request&#x2026;</p>
      </div>
    );
  }

  if (phase === "invalid" || phase === "not_found") {
    return (
      <Terminal title="This sign-in link isn't valid">
        The request link is malformed or no longer exists. Start the sign-in again from the app on
        your device.
      </Terminal>
    );
  }

  if (phase === "expired") {
    return (
      <Terminal title="This request expired">
        Sign-in requests only last a few minutes. Start a new one from the app on your device.
      </Terminal>
    );
  }

  if (phase === "fulfilled") {
    return (
      <Terminal title="This request was already handled">
        It was completed once already. If that wasn't you, start a fresh sign-in from your device.
      </Terminal>
    );
  }

  if (phase === "unsupported") {
    return <Terminal title="This request can't be approved">{error}</Terminal>;
  }

  if (phase === "load_error") {
    return (
      <Terminal title="Couldn't load the request">
        {error ?? "Something went wrong reaching the sign-in server."}
      </Terminal>
    );
  }

  if (phase === "different_account") {
    return (
      <Terminal title="Wrong wallet connected">
        This request is for{" "}
        <span style={chip}>{shorten(request?.sender ?? "")}</span>. Switch to that account in your
        wallet, then reopen this link.
      </Terminal>
    );
  }

  if (phase === "denied") {
    return (
      <Terminal title="You declined the request">
        Nothing was signed or sent. You can close this tab.
      </Terminal>
    );
  }

  if (phase === "done") {
    return (
      <Terminal title="Approved">
        {loaded.isDeepLink
          ? "You're all set \u{2014} return to the app on your device. You can close this tab."
          : "You're all set. Return to the app that asked for this. You can close this tab."}
      </Terminal>
    );
  }

  if (phase === "error") {
    return (
      <Terminal title="The request couldn't be completed">
        {error ?? "The wallet reported an error."}
      </Terminal>
    );
  }

  if (!request) return null;

  const summary = describeRequest(request);
  const isSigning = phase === "signing";

  return (
    <div style={card}>
      <h2 style={{ margin: "0 0 4px" }}>Approve this request</h2>
      <p style={subtle}>
        The app on your device is asking your wallet to approve the action below on{" "}
        <strong>{loaded.host}</strong>.
      </p>

      <div style={codeBox}>
        <div style={codeDigits}>{String(request.code).padStart(2, "0")}</div>
        <div style={codeLabel}>Verification code</div>
      </div>
      <p style={{ ...subtle, fontSize: 14, marginTop: 0 }}>
        Only continue if this code matches the one shown on your device. If it doesn't, close this
        tab {"\u{2014}"} someone may be trying to trick you.
      </p>

      <p style={{ ...subtle, fontWeight: 600, color: "#fcfcfc", margin: "18px 0 4px", textAlign: "left" }}>
        {summary.title}
      </p>
      <pre style={detailBox}>{summary.detail}</pre>

      {request.sender ? (
        <p style={{ ...subtle, fontSize: 13 }}>
          For account <span style={chip}>{shorten(request.sender)}</span>
        </p>
      ) : null}

      {remaining !== null ? (
        <p style={{ ...subtle, fontSize: 13 }}>Expires in {formatCountdown(remaining)}</p>
      ) : null}

      {mustValidate ? (
        <label style={ackRow}>
          <input
            type="checkbox"
            checked={acknowledged}
            onChange={(event) => setAcknowledged(event.target.checked)}
            style={{ marginTop: 3 }}
          />
          <span>I confirm the code above matches the one shown on my device.</span>
        </label>
      ) : null}

      {error ? (
        <p style={{ ...subtle, color: "#ff5c77", marginTop: 12 }} role="alert">
          {error}
        </p>
      ) : null}

      <button
        style={button}
        onClick={onApprove}
        disabled={isSigning || (mustValidate && !acknowledged)}
      >
        {isSigning ? "Waiting for your wallet\u{2026}" : "Approve in wallet"}
      </button>
      <button style={denyButton} onClick={onDeny} disabled={isSigning}>
        Deny
      </button>

      {walletName === null ? (
        <p style={{ ...subtle, fontSize: 13, marginTop: 12 }}>
          No wallet detected in this browser. Approving will prompt you to connect one.
        </p>
      ) : null}

      <p style={brandFooter}>
        This page approves a single request from a Decentraland app. Your wallet key never leaves
        your wallet.
      </p>
    </div>
  );
}

function Terminal({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div style={card}>
      <h2>{title}</h2>
      <p style={subtle}>{children}</p>
    </div>
  );
}

function shorten(address: string): string {
  if (!address || address.length < 12) return address;
  return `${address.slice(0, 6)}\u{2026}${address.slice(-4)}`;
}

function isUserRejection(err: unknown): boolean {
  if (!err || typeof err !== "object") return false;
  const code = (err as { code?: number }).code;
  if (code === 4001 || code === -32003) return true;
  const message = (err as { message?: string }).message ?? "";
  return /user rejected|user denied|rejected the request/i.test(message);
}
