import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { data, useLoaderData } from "react-router";

import { isIdentityExpired } from "@data/lib/auth/expiry";
import type { AuthIdentity } from "@data/lib/auth/types";
import {
  connectWallet,
  getConnectedAddress,
  hasWallet,
  selectWallet,
  walletProvider,
} from "@data/lib/auth/wallet";

import {
  completeDeepLinkSignIn,
  getAuthRequestId,
  getExplorerDeeplink,
  getSigninDeeplink,
  isBridgeOnlyEnabled,
  isDeepLinkFlowEnabled,
  isValidUuidV4,
  launchDeepLink,
  resolveAuthApiUrl,
  startsRequestRecovery,
} from "../lib/auth-deeplink";
import { runExclusive } from "../lib/auth-reentrancy";
import {
  RPC_USER_REJECTED,
  approveBlocked,
  buildWalletRequest,
  decodeSignatureMessage,
  denialSender,
  isSameAccount,
  previewTransaction,
  rejectionOutcome,
  type RejectionKind,
  type RequestRejection,
  type UnverifiableReason,
} from "../lib/auth-request-params";
import {
  recoverAuthRequest,
  type LoadResult,
  type OutcomeError,
  type ReadyRequest,
  type RecoverResponse,
} from "../lib/auth-request-recovery";
import {
  escapeUnreadableTypedDataText,
  sanitizeTypedDataForDisplay,
  truncateForDisplay,
} from "../lib/auth-typed-data-escape";

import type { Route } from "./+types/auth.requests.$id";

export function meta(_args: Route.MetaArgs) {
  return [{ title: "Approve sign-in request \u{2014} Decentraland" }];
}

const AUTH_API = "/auth-api";
const ID_RE = /^[0-9a-fA-F-]{30,80}$/;
const NO_STORE = { headers: { "cache-control": "no-store" } };

type LoadedRequest = {
  id: string;
  valid: boolean;
  host: string;
  loginMethod: string;
  isDeepLink: boolean;
  deepLinkIdValid: boolean;
  bridgeOnly: boolean;
  authRequestId: string | null;
  authApiUrl: string;
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const id = params.id ?? "";
  const loaded: LoadedRequest = {
    id,
    valid: ID_RE.test(id),
    host: url.host,
    loginMethod: url.searchParams.get("loginMethod") ?? "",
    isDeepLink: isDeepLinkFlowEnabled(url.searchParams),
    deepLinkIdValid: isValidUuidV4(id),
    bridgeOnly: isBridgeOnlyEnabled(url.searchParams),
    authRequestId: getAuthRequestId(url.searchParams),
    authApiUrl: resolveAuthApiUrl(process.env.AUTH_API_URL, url.host),
  };
  return data(loaded, NO_STORE);
}

const INVALID_DEEP_LINK_ID_MESSAGE = "The sign-in link is invalid.";

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

const RDNS_BY_METHOD: Record<string, string> = {
  metamask: "io.metamask",
  coinbase: "com.coinbase.wallet",
  rabby: "io.rabby",
};

// What the rejection means for the person holding the phone; the technical reason rides along
// underneath so a developer can see which rule the request broke.
const REJECTION_COPY: Record<RejectionKind, string> = {
  retired_sign_in: "This request uses a retired sign-in method. Update the app that opened it.",
  unsupported_method: "This request uses a method this page can't hand to a wallet.",
  impersonated_sign_in:
    "This request tried to sign a Decentraland identity payload and was blocked.",
  malformed_signature:
    "This request's parameters don't match what your wallet would sign, so it was blocked.",
  malformed_transaction:
    "This request's transaction parameters aren't something your wallet can execute as shown, so it was blocked.",
};

const UNVERIFIABLE_NOTICE: Record<UnverifiableReason, string> = {
  opaque_message:
    "This message doesn't look like readable text, so you can't check what you're signing. It could be a hash that authorizes an action in a contract. Only sign it if you trust the app that requested it.",
  unrecognized_typed_data:
    "This page can't preview what this signature authorizes. Only sign it if you trust the app that requested it.",
  unsimulated_transaction:
    "This page can't preview what this transaction will do with your assets. Only send it if you trust the app that requested it.",
};

// Nothing signed reaches the page as itself: a character that would reorder, hide or merge with its
// neighbours is shown as an escape, so what is read here is what the wallet is handed.
function typedDataDetail(value: unknown): string {
  if (typeof value === "string") {
    try {
      return truncateForDisplay(
        JSON.stringify(sanitizeTypedDataForDisplay(JSON.parse(value)), null, 2),
      );
    } catch {
      return truncateForDisplay(escapeUnreadableTypedDataText(value));
    }
  }
  return truncateForDisplay(JSON.stringify(sanitizeTypedDataForDisplay(value), null, 2));
}

export type RequestSummary = { title: string; detail: string; note?: string };

export function describeRequest(request: ReadyRequest): RequestSummary {
  switch (request.method) {
    case "personal_sign":
      return {
        title: "Sign a message",
        detail: decodeSignatureMessage(request.params[0]) ?? JSON.stringify(request.params, null, 2),
      };
    case "eth_signTypedData_v3":
    case "eth_signTypedData_v4":
      return { title: "Sign typed data", detail: typedDataDetail(request.params[1]) };
    case "eth_sendTransaction": {
      const { shown, dropped } = previewTransaction(request.params);
      return {
        title: "Send a transaction",
        detail: JSON.stringify(shown, null, 2),
        note:
          dropped.length > 0
            ? `Not sent to the wallet: ${dropped.map(escapeUnreadableTypedDataText).join(", ")}`
            : undefined,
      };
    }
  }
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
  | "error"
  | "deeplink_ready"
  | "deeplink_signing"
  | "continue_in_app"
  | "client_login_error";

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

// Keyed by the request id so a client-side change of id mounts a fresh page: no state, ref or
// in-flight recovery of the previous request can be shown or answered under the new id.
export default function AuthRequestRoute() {
  const loaded = useLoaderData<typeof loader>() as LoadedRequest;
  return <AuthRequestPage key={loaded.id} loaded={loaded} />;
}

export function AuthRequestPage({ loaded }: { loaded: LoadedRequest }) {
  const [phase, setPhase] = useState<Phase>(() => {
    if (loaded.isDeepLink) {
      return loaded.deepLinkIdValid ? "deeplink_ready" : "client_login_error";
    }
    return loaded.valid ? "loading" : "invalid";
  });
  const [request, setRequest] = useState<ReadyRequest | null>(null);
  const [rejection, setRejection] = useState<RequestRejection | null>(null);
  const [mustValidate, setMustValidate] = useState(false);
  const [acknowledged, setAcknowledged] = useState(false);
  const [unverifiable, setUnverifiable] = useState<UnverifiableReason | null>(null);
  const [effectsAcknowledged, setEffectsAcknowledged] = useState(false);
  const [remaining, setRemaining] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(
    loaded.isDeepLink && !loaded.deepLinkIdValid ? INVALID_DEEP_LINK_ID_MESSAGE : null,
  );
  const [expectedAccount, setExpectedAccount] = useState<string | null>(null);
  const [walletName, setWalletName] = useState<string | null>(null);
  const [identityId, setIdentityId] = useState<string | null>(null);
  const startedRef = useRef(false);
  const deepLinkIdentityRef = useRef<AuthIdentity | null>(null);
  const isApprovingRef = useRef(false);
  const isDenyingRef = useRef(false);
  const isDeepLinkSigningRef = useRef(false);

  useEffect(() => {
    if (!startsRequestRecovery(loaded) || startedRef.current) return;
    startedRef.current = true;
    let cancelled = false;

    void (async () => {
      const outcome = await recoverAuthRequest(loaded.id, {
        load: loadRequest,
        requiresValidation,
        connectedAddress: () => getConnectedAddress().catch(() => null),
        postOutcome,
      });
      if (cancelled) return;

      switch (outcome.kind) {
        case "ready":
          setRequest(outcome.request);
          setMustValidate(outcome.needsValidation);
          setUnverifiable(outcome.unverifiable);
          setPhase("ready");
          return;
        case "rejected":
          setRejection(outcome.rejection);
          setPhase("unsupported");
          return;
        case "error":
          setError(outcome.message);
          setPhase("load_error");
          return;
        default:
          setPhase(outcome.kind);
      }
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
    if (!request) return;
    if (
      approveBlocked({
        isSigning: phase === "signing",
        mustValidate,
        acknowledged,
        unverifiable,
        effectsAcknowledged,
      })
    ) {
      return;
    }

    await runExclusive(isApprovingRef, async () => {
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

      if (request.sender && !isSameAccount(request.sender, sender)) {
        setExpectedAccount(request.sender);
        setPhase("different_account");
        return;
      }

      // The signer element was only shape-checked at recover when the request named no sender;
      // now that an account is connected it is held to the exact rule, and the transaction is
      // rebuilt from the reviewed fields alone.
      const wallet = buildWalletRequest(request, sender);
      if (!wallet.ok) {
        await postOutcome(loaded.id, {
          sender,
          error: rejectionOutcome(wallet.rejection),
        }).catch(() => null);
        setRejection(wallet.rejection);
        setPhase("unsupported");
        return;
      }

      if (mustValidate && !(await requiresValidation(loaded.id))) {
        setMustValidate(false);
      }

      // The wallet can move to another account while the code-match check is in flight. Only the
      // account this request's signer rule was applied to may be handed it, and an account this page
      // can no longer read is not that account: nothing is dispatched and nothing is answered, so the
      // request stays available for whoever reopens the link.
      if (!isSameAccount(await getConnectedAddress().catch(() => null), sender)) {
        setExpectedAccount(sender);
        setPhase("different_account");
        return;
      }

      let executed = false;
      try {
        const result = await walletProvider().request(wallet.request);
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
            error: { code: RPC_USER_REJECTED, message: "Request rejected" },
          }).catch(() => null);
          setPhase("denied");
          return;
        }
        const message = (err as Error)?.message ?? "The wallet couldn't complete the request.";
        await postOutcome(loaded.id, { sender, error: { code: 999, message } }).catch(() => null);
        setError(message);
        setPhase("error");
      }
    });
  }, [
    request,
    phase,
    mustValidate,
    acknowledged,
    unverifiable,
    effectsAcknowledged,
    loaded.id,
    loaded.loginMethod,
  ]);

  const onDeny = useCallback(async () => {
    if (!request) return;

    await runExclusive(isDenyingRef, async () => {
      setPhase("denied");
      const sender = denialSender(request, await getConnectedAddress().catch(() => null));
      if (sender) {
        await postOutcome(loaded.id, {
          sender,
          error: { code: RPC_USER_REJECTED, message: "Request rejected" },
        }).catch(() => null);
      }
    });
  }, [request, loaded.id]);

  const onDeepLinkSignIn = useCallback(async () => {
    if (phase === "deeplink_signing") return;

    await runExclusive(isDeepLinkSigningRef, async () => {
      setError(null);
      setPhase("deeplink_signing");

      const rdns = RDNS_BY_METHOD[loaded.loginMethod.toLowerCase()] ?? null;
      const [{ createIdentityFor }, { postIdentityHandoff, DEEPLINK_IDENTITY_EXPIRATION_MS }] =
        await Promise.all([
          import("@data/lib/auth/identity"),
          import("@data/lib/auth/deeplink-identity"),
        ]);

      // Only the identity minted on this page counts as cached (so a failed post retries without
      // a second signature); the site's session identity is never handed to the auth server, and
      // the handoff identity is never stored as the site session -- one extra wallet signature per
      // deep-link login buys that separation.
      const outcome = await completeDeepLinkSignIn({
        connect: async () => {
          selectWallet(rdns);
          return (await getConnectedAddress()) ?? (await connectWallet());
        },
        cachedIdentity: (sender) => {
          const candidate = deepLinkIdentityRef.current;
          return candidate &&
            candidate.signer.toLowerCase() === sender.toLowerCase() &&
            !isIdentityExpired(candidate)
            ? candidate
            : null;
        },
        createIdentity: (sender) =>
          createIdentityFor(sender, { expirationMs: DEEPLINK_IDENTITY_EXPIRATION_MS }),
        postIdentity: (identity) => postIdentityHandoff(identity, loaded.authApiUrl),
        isUserRejection,
      });

      switch (outcome.kind) {
        case "ok":
          deepLinkIdentityRef.current = outcome.identity;
          setIdentityId(outcome.identityId);
          setPhase("continue_in_app");
          return;
        case "denied":
          setPhase("denied");
          return;
        case "wallet_error":
          setError(outcome.message);
          setPhase("load_error");
          return;
        case "post_error":
          deepLinkIdentityRef.current = outcome.identity;
          setError(outcome.message);
          setPhase("client_login_error");
          return;
      }
    });
  }, [phase, loaded.loginMethod, loaded.authApiUrl]);

  const onRetryClientLogin = useCallback(() => {
    if (loaded.deepLinkIdValid) {
      void onDeepLinkSignIn();
    } else {
      window.location.reload();
    }
  }, [loaded.deepLinkIdValid, onDeepLinkSignIn]);

  const onReturnToExplorer = useCallback(() => {
    window.location.href = getExplorerDeeplink(undefined, loaded.bridgeOnly, loaded.authRequestId);
  }, [loaded.bridgeOnly, loaded.authRequestId]);

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
    return (
      <Terminal title="This request can't be approved">
        {rejection ? REJECTION_COPY[rejection.kind] : error}
        {rejection ? (
          <>
            <br />
            <span style={{ ...chip, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
              {rejection.message}
            </span>
          </>
        ) : null}
      </Terminal>
    );
  }

  if (phase === "load_error") {
    return (
      <Terminal
        title="Couldn't load the request"
        action={
          loaded.isDeepLink ? (
            <button style={button} onClick={onReturnToExplorer}>
              Try again
            </button>
          ) : null
        }
      >
        {error ?? "Something went wrong reaching the sign-in server."}
      </Terminal>
    );
  }

  if (phase === "client_login_error") {
    return (
      <Terminal
        title="Couldn't complete the sign-in"
        action={
          <button style={button} onClick={onRetryClientLogin}>
            Try again
          </button>
        }
      >
        Something went wrong handing your sign-in to the app.
        {error ? (
          <>
            <br />
            <span style={{ color: "#ff5c77" }} role="alert">
              {error}
            </span>
          </>
        ) : null}
      </Terminal>
    );
  }

  if (phase === "continue_in_app") {
    return (
      <ContinueInApp
        deepLinkUrl={getSigninDeeplink(
          undefined,
          identityId ?? "",
          loaded.bridgeOnly,
          loaded.id,
        )}
      />
    );
  }

  if (phase === "deeplink_ready" || phase === "deeplink_signing") {
    const isSigningIn = phase === "deeplink_signing";
    return (
      <div style={card}>
        <h2 style={{ margin: "0 0 4px" }}>Sign in to Decentraland</h2>
        <p style={subtle}>
          The Decentraland app on this device is waiting for you to sign in with your wallet on{" "}
          <strong>{loaded.host}</strong>.
        </p>
        <p style={{ ...subtle, fontSize: 14 }}>
          Your wallet will ask you to sign a message that creates a session for the app. It costs
          nothing and moves no funds.
        </p>

        {error ? (
          <p style={{ ...subtle, color: "#ff5c77", marginTop: 12 }} role="alert">
            {error}
          </p>
        ) : null}

        <button style={button} onClick={onDeepLinkSignIn} disabled={isSigningIn}>
          {isSigningIn ? "Waiting for your wallet\u{2026}" : "Sign in with wallet"}
        </button>

        {walletName === null ? (
          <p style={{ ...subtle, fontSize: 13, marginTop: 12 }}>
            No wallet detected in this browser. Signing in will prompt you to connect one.
          </p>
        ) : null}

        <p style={brandFooter}>
          This page signs you in to a Decentraland app on this device. Your wallet key never
          leaves your wallet.
        </p>
      </div>
    );
  }

  if (phase === "different_account") {
    return (
      <Terminal title="Wrong wallet connected">
        This request is for{" "}
        <span style={chip}>{shorten(expectedAccount ?? request?.sender ?? "")}</span>. Switch to
        that account in your wallet, then reopen this link.
      </Terminal>
    );
  }

  if (phase === "denied") {
    return (
      <Terminal title="You declined the request">
        {loaded.isDeepLink
          ? "Nothing was signed. Return to the app on your device and start the sign-in again."
          : "Nothing was signed or sent. You can close this tab."}
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

  return (
    <ApprovalCard
      host={loaded.host}
      request={request}
      remaining={remaining}
      mustValidate={mustValidate}
      acknowledged={acknowledged}
      onAcknowledged={setAcknowledged}
      unverifiable={unverifiable}
      effectsAcknowledged={effectsAcknowledged}
      onEffectsAcknowledged={setEffectsAcknowledged}
      error={error}
      isSigning={phase === "signing"}
      walletDetected={walletName !== null}
      onApprove={onApprove}
      onDeny={onDeny}
    />
  );
}

type ApprovalCardProps = {
  host: string;
  request: ReadyRequest;
  remaining: number | null;
  mustValidate: boolean;
  acknowledged: boolean;
  onAcknowledged: (checked: boolean) => void;
  unverifiable: UnverifiableReason | null;
  effectsAcknowledged: boolean;
  onEffectsAcknowledged: (checked: boolean) => void;
  error: string | null;
  isSigning: boolean;
  walletDetected: boolean;
  onApprove: () => void;
  onDeny: () => void;
};

// Two independent gates in front of Approve: the code match the auth server asked for, and the
// effects acknowledgment for anything this page cannot preview. Neither substitutes for the other.
export function ApprovalCard({
  host,
  request,
  remaining,
  mustValidate,
  acknowledged,
  onAcknowledged,
  unverifiable,
  effectsAcknowledged,
  onEffectsAcknowledged,
  error,
  isSigning,
  walletDetected,
  onApprove,
  onDeny,
}: ApprovalCardProps) {
  const summary = describeRequest(request);
  const isTransaction = request.method === "eth_sendTransaction";
  const blocked = approveBlocked({
    isSigning,
    mustValidate,
    acknowledged,
    unverifiable,
    effectsAcknowledged,
  });

  return (
    <div style={card}>
      <h2 style={{ margin: "0 0 4px" }}>Approve this request</h2>
      <p style={subtle}>
        The app on your device is asking your wallet to approve the action below on{" "}
        <strong>{host}</strong>.
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
      {summary.note ? (
        <p style={{ ...subtle, fontSize: 13, textAlign: "left", margin: "-8px 2px 16px" }}>
          {summary.note}
        </p>
      ) : null}

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
            onChange={(event) => onAcknowledged(event.target.checked)}
            style={{ marginTop: 3 }}
          />
          <span>I confirm the code above matches the one shown on my device.</span>
        </label>
      ) : null}

      {unverifiable !== null ? (
        <>
          <p style={{ ...subtle, fontSize: 14, textAlign: "left", margin: "14px 2px 0" }} role="note">
            {UNVERIFIABLE_NOTICE[unverifiable]}
          </p>
          <label style={ackRow}>
            <input
              type="checkbox"
              checked={effectsAcknowledged}
              onChange={(event) => onEffectsAcknowledged(event.target.checked)}
              style={{ marginTop: 3 }}
            />
            <span>
              {`I understand the effects of this ${isTransaction ? "transaction" : "signature"} couldn't be verified.`}
            </span>
          </label>
        </>
      ) : null}

      {error ? (
        <p style={{ ...subtle, color: "#ff5c77", marginTop: 12 }} role="alert">
          {error}
        </p>
      ) : null}

      <button style={button} onClick={onApprove} disabled={blocked}>
        {isSigning ? "Waiting for your wallet\u{2026}" : "Approve in wallet"}
      </button>
      <button style={denyButton} onClick={onDeny} disabled={isSigning}>
        Deny
      </button>

      {!walletDetected ? (
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

function Terminal({
  title,
  children,
  action,
}: {
  title: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div style={card}>
      <h2>{title}</h2>
      <p style={subtle}>{children}</p>
      {action}
    </div>
  );
}

const CONTINUE_COUNTDOWN_SECONDS = 5;

// Ported from auth's ContinueInApp view: the client is opened once the countdown ends, the
// button stays for browsers that only hand a custom scheme to the OS on a user gesture, and a
// launch that never took the focus away offers a retry instead of a dead end.
function ContinueInApp({ deepLinkUrl }: { deepLinkUrl: string }) {
  const [countdown, setCountdown] = useState(CONTINUE_COUNTDOWN_SECONDS);
  const [deepLinkFailed, setDeepLinkFailed] = useState(false);
  const [hasLaunched, setHasLaunched] = useState(false);
  const isLaunchingRef = useRef(false);

  const attemptDeepLink = useCallback(async () => {
    if (isLaunchingRef.current) return;
    isLaunchingRef.current = true;
    try {
      const wasLaunched = await launchDeepLink(deepLinkUrl);
      if (wasLaunched) {
        setHasLaunched(true);
      } else {
        setDeepLinkFailed(true);
      }
    } finally {
      isLaunchingRef.current = false;
    }
  }, [deepLinkUrl]);

  const handleRetry = useCallback(() => {
    setCountdown(CONTINUE_COUNTDOWN_SECONDS);
    setDeepLinkFailed(false);
  }, []);

  useEffect(() => {
    if (deepLinkFailed || hasLaunched) return;
    const interval = setInterval(() => {
      setCountdown((prev) => (prev <= 0 ? 0 : prev - 1));
    }, 1000);
    return () => clearInterval(interval);
  }, [deepLinkFailed, hasLaunched]);

  useEffect(() => {
    if (deepLinkFailed || hasLaunched) return;
    if (countdown === 0) void attemptDeepLink();
  }, [attemptDeepLink, countdown, deepLinkFailed, hasLaunched]);

  if (deepLinkFailed) {
    return (
      <div style={card}>
        <h2>Couldn't open Decentraland</h2>
        <p style={subtle}>
          The app didn't launch. Make sure Decentraland is installed on this device, then try
          again.
        </p>
        <button style={button} onClick={handleRetry}>
          Try again
        </button>
        <p style={brandFooter}>
          <a href={deepLinkUrl} style={{ color: "inherit" }}>
            Open the link directly
          </a>
        </p>
      </div>
    );
  }

  return (
    <div style={card}>
      <h2>Sign-in successful</h2>
      <p style={subtle}>
        {hasLaunched
          ? "Redirecting you to Decentraland\u{2026} You can close this tab once the app opens."
          : `Redirecting you to Decentraland in ${countdown}\u{2026}`}
      </p>
      <button style={button} onClick={() => void attemptDeepLink()}>
        Return to Decentraland
      </button>
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
