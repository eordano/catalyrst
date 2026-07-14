import type { AuthIdentity } from "@data/lib/auth/types";

// Mirrors auth/src/shared/locations.ts: the flags the request page can be opened with, and
// which the client deep link carries back.
export const FLOW_PARAM = "flow";
export const DEEP_LINK_FLOW_VALUE = "deeplink";
export const BRIDGE_ONLY_PARAM = "bridgeOnly";
export const AUTH_REQUEST_ID_PARAM = "authRequestId";
export const EXPLORER_DEEP_LINK = "decentraland://";

const UUID_V4_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export function isValidUuidV4(value: string): boolean {
  return UUID_V4_RE.test(value);
}

export function isDeepLinkFlowEnabled(params: URLSearchParams): boolean {
  return (params.get(FLOW_PARAM) ?? "").toLowerCase() === DEEP_LINK_FLOW_VALUE;
}

export function isBridgeOnlyEnabled(params: URLSearchParams): boolean {
  if (!params.has(BRIDGE_ONLY_PARAM)) return false;
  const value = (params.get(BRIDGE_ONLY_PARAM) ?? "").toLowerCase();
  return value === "" || value === "true";
}

export function getAuthRequestId(params: URLSearchParams): string | null {
  return params.get(AUTH_REQUEST_ID_PARAM);
}

// The deep-link handoff has no backing auth-server request: a client-generated UUID never
// resolves on /v2/requests/:id, so the recover/verify half must not start for it.
export function startsRequestRecovery(loaded: { isDeepLink: boolean; valid: boolean }): boolean {
  return !loaded.isDeepLink && loaded.valid;
}

// Upstream reads ENVIRONMENT through @dcl/ui-env, which maps the page's TLD (.org/.co ->
// production, .today/.net -> staging, .io/.zone -> development) and otherwise falls back to the
// build default, production. catalyst.example.com and any self-hosted twin match no TLD rule, so this is
// production: no dclenv rides on the deep link and the client keeps the environment it was
// launched with (a --base-domain client ignores dclenv anyway).
export const ENVIRONMENT = "production";

export function getDeeplinkQueryParams(
  bridgeOnly?: boolean,
  authRequestId?: string | null,
): URLSearchParams {
  const env = ENVIRONMENT.toLowerCase();
  const params = new URLSearchParams();
  if (env !== "production") {
    params.set("dclenv", env === "development" ? "zone" : env);
  }
  if (bridgeOnly) params.set("bridgeOnly", "true");
  if (authRequestId) params.set("authRequestId", authRequestId);
  return params;
}

export function getExplorerDeeplink(
  deepLink?: string,
  bridgeOnly?: boolean,
  authRequestId?: string | null,
): string {
  const base = deepLink || EXPLORER_DEEP_LINK;
  const query = getDeeplinkQueryParams(bridgeOnly, authRequestId).toString();
  return query ? `${base}?${query}` : base;
}

export function getSigninDeeplink(
  deepLink: string | undefined,
  identityId: string,
  bridgeOnly?: boolean,
  authRequestId?: string | null,
): string {
  const params = new URLSearchParams({ signin: identityId });
  getDeeplinkQueryParams(bridgeOnly, authRequestId).forEach((value, key) =>
    params.set(key, value),
  );
  return `${deepLink || EXPLORER_DEEP_LINK}open?${params.toString()}`;
}

// The identity goes to the same host the client reads it back from: the client derives
// ApiAuth as https://auth-api.{BaseDomain} (DecentralandUrlsSource.cs), so the fanout vhost is
// the default target. That derivation assumes the page host IS the registrable domain, the way
// the client's --base-domain does; a site fronted anywhere else (www., app.example.com) must
// set AUTH_API_URL, see resolveAuthApiUrl. A dev origin (localhost, or any host carrying a
// port) has no fanout twin, so it keeps the same-origin /auth-api prefix the polling flow uses,
// which the dev server proxies to the catalyst apex (01-catalyst.conf mounts /identities too).
export function authApiUrlFor(host: string): string {
  const bare = host.toLowerCase();
  if (!bare || bare.includes(":") || bare === "localhost" || bare.endsWith(".localhost")) {
    return "/auth-api";
  }
  return `https://auth-api.${bare}`;
}

export function resolveAuthApiUrl(configured: string | undefined, host: string): string {
  const explicit = (configured ?? "").trim().replace(/\/+$/, "");
  return explicit || authApiUrlFor(host);
}

export function isMobileUserAgent(userAgent: string): boolean {
  return /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(userAgent);
}

// Native-protocol confirmation dialogs need time for the user to react: a shorter window
// renders the failure view while the browser prompt is still open, and the app then launches
// after the user accepts it.
export const DEEPLINK_DETECTION_TIMEOUT = 5000;

// The hidden iframe keeps Safari from navigating the tab to an unhandled scheme; blur,
// pagehide and a hidden document are the only signals that an app took over.
export function launchDeepLink(url: string): Promise<boolean> {
  return new Promise((resolve) => {
    if (isMobileUserAgent(navigator.userAgent)) {
      window.location.href = url;
      resolve(true);
      return;
    }

    const iframe = document.createElement("iframe");
    iframe.setAttribute("style", "display: none");
    iframe.src = url;

    let settled = false;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;

    const cleanup = () => {
      window.removeEventListener("blur", handleAppLaunch);
      window.removeEventListener("pagehide", handleAppLaunch);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      if (timeoutId !== undefined) clearTimeout(timeoutId);
      iframe.remove();
    };

    const settle = (wasLaunched: boolean) => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve(wasLaunched);
    };

    const handleAppLaunch = () => settle(true);
    const handleVisibilityChange = () => {
      if (document.visibilityState === "hidden") settle(true);
    };

    window.addEventListener("blur", handleAppLaunch);
    window.addEventListener("pagehide", handleAppLaunch);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    timeoutId = setTimeout(() => settle(false), DEEPLINK_DETECTION_TIMEOUT);
    document.body.appendChild(iframe);
  });
}

export type DeepLinkSignInOutcome =
  | { kind: "ok"; identityId: string; identity: AuthIdentity }
  | { kind: "denied" }
  | { kind: "wallet_error"; message: string }
  | { kind: "post_error"; message: string; identity: AuthIdentity };

export type DeepLinkSignInDeps = {
  connect: () => Promise<string>;
  cachedIdentity: (sender: string) => AuthIdentity | null;
  createIdentity: (sender: string) => Promise<AuthIdentity>;
  postIdentity: (identity: AuthIdentity) => Promise<{ identityId: string }>;
  isUserRejection: (err: unknown) => boolean;
};

function messageOf(err: unknown, fallback: string): string {
  const message = (err as { message?: unknown } | null)?.message;
  return typeof message === "string" && message ? message : fallback;
}

// The client-login half of upstream's RequestPage (completeClientLoginFlow): there is no
// auth-server request to recover, the connected wallet's identity is posted once and the
// resulting id rides back to the client on the signin deep link. The wallet steps upstream
// runs on its login page (connect + sign the ephemeral message) happen here instead. The
// handoff identity is minted fresh for every first attempt and lives only in the caller's
// memory for a retry: the site's own session identity is never posted (the auth server hands
// its ephemeral key to whoever GETs the record first) and never replaced by the 30-day handoff
// one, at the price of one wallet signature per deep-link login.
export async function completeDeepLinkSignIn(
  deps: DeepLinkSignInDeps,
): Promise<DeepLinkSignInOutcome> {
  let sender: string;
  try {
    sender = await deps.connect();
  } catch (err) {
    if (deps.isUserRejection(err)) return { kind: "denied" };
    return { kind: "wallet_error", message: messageOf(err, "Couldn't connect your wallet.") };
  }

  let identity = deps.cachedIdentity(sender);
  if (!identity) {
    try {
      identity = await deps.createIdentity(sender);
    } catch (err) {
      if (deps.isUserRejection(err)) return { kind: "denied" };
      return { kind: "wallet_error", message: messageOf(err, "Your wallet couldn't sign in.") };
    }
  }

  try {
    const { identityId } = await deps.postIdentity(identity);
    return { kind: "ok", identityId, identity };
  } catch (err) {
    return { kind: "post_error", message: messageOf(err, "Unknown error"), identity };
  }
}
