import { renderToString } from "react-dom/server";
import {
  createStaticHandler,
  createStaticRouter,
  StaticRouterProvider,
  type LoaderFunction,
} from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import {
  buildWalletRequest,
  unverifiableReason,
  type AllowedMethod,
} from "../lib/auth-request-params";
import type { ReadyRequest } from "../lib/auth-request-recovery";

import AuthRequestRoute, { ApprovalCard, describeRequest, loader } from "./auth.requests.$id";

const REQUEST_ID = "123e4567-e89b-42d3-a456-426614174000";
const SENDER = "0x1234567890abcdef1234567890abcdef12345678";
const TO = "0xfef5c99885c3036e591b6e6db52482891834a5f4";
const PERMIT = JSON.stringify({
  domain: { name: "Token", version: "1", chainId: 137 },
  primaryType: "Permit",
  types: { Permit: [{ name: "spender", type: "address" }] },
  message: { spender: "0x000000000000000000000000000000000000dead" },
});
const EFFECTS_LABEL = "couldn&#x27;t be verified";

const routes = [
  {
    path: "/auth/requests/:id",
    Component: AuthRequestRoute,
    loader: loader as unknown as LoaderFunction,
  },
];

// The real loader feeds the real first paint, the way the SSR entry does: no effect has run yet,
// so what shows is exactly what the URL alone decides.
async function firstPaint(path: string): Promise<{ data: Record<string, unknown>; html: string }> {
  const handler = createStaticHandler(routes);
  const context = await handler.query(new Request(`https://catalyst.example.com${path}`));
  if (context instanceof Response) throw new Error(`unexpected response ${context.status}`);
  const router = createStaticRouter(handler.dataRoutes, context);
  const html = renderToString(<StaticRouterProvider router={router} context={context} />);
  const data = context.loaderData[handler.dataRoutes[0]!.id] as Record<string, unknown>;
  return { data, html };
}

describe("loader", () => {
  const configured = process.env.AUTH_API_URL;
  afterEach(() => {
    if (configured === undefined) delete process.env.AUTH_API_URL;
    else process.env.AUTH_API_URL = configured;
  });

  it("targets AUTH_API_URL when the deployment sets one", async () => {
    process.env.AUTH_API_URL = "https://auth.example.com/";
    const { data } = await firstPaint(`/auth/requests/${REQUEST_ID}?flow=deeplink`);
    expect(data.authApiUrl).toBe("https://auth.example.com");
  });

  it("parses the flags the client puts on the deep-link sign-in URL", async () => {
    delete process.env.AUTH_API_URL;
    const { data } = await firstPaint(
      `/auth/requests/${REQUEST_ID}?loginMethod=Metamask&flow=deeplink&bridgeOnly`,
    );
    expect(data.isDeepLink).toBe(true);
    expect(data.deepLinkIdValid).toBe(true);
    expect(data.bridgeOnly).toBe(true);
    expect(data.authRequestId).toBeNull();
    expect(data.loginMethod).toBe("Metamask");
    expect(data.authApiUrl).toBe("https://auth-api.catalyst.example.com");
  });

  it("flags a malformed deep-link id", async () => {
    const { data } = await firstPaint("/auth/requests/not-a-uuid?flow=deeplink");
    expect(data.isDeepLink).toBe(true);
    expect(data.deepLinkIdValid).toBe(false);
  });

  it("forwards an explicit authRequestId query value for the bare explorer link", async () => {
    const { data } = await firstPaint(
      `/auth/requests/${REQUEST_ID}?flow=deeplink&authRequestId=req-7`,
    );
    expect(data.authRequestId).toBe("req-7");
  });

  it("leaves the polling flow's flags untouched", async () => {
    const { data } = await firstPaint(`/auth/requests/${REQUEST_ID}?loginMethod=metamask`);
    expect(data.isDeepLink).toBe(false);
    expect(data.bridgeOnly).toBe(false);
    expect(data.valid).toBe(true);
    expect(data.loginMethod).toBe("metamask");
  });
});

describe("first paint", () => {
  it("offers the wallet sign-in for the deep-link flow instead of loading a request", async () => {
    const { html } = await firstPaint(`/auth/requests/${REQUEST_ID}?loginMethod=Metamask&flow=deeplink`);
    expect(html).toContain("Sign in to Decentraland");
    expect(html).toContain("Sign in with wallet");
    expect(html).not.toContain("Loading the sign-in request");
  });

  it("rejects a deep-link id that is not a UUID v4 with the client-login error", async () => {
    const { html } = await firstPaint("/auth/requests/not-a-uuid?flow=deeplink");
    expect(html).toContain("complete the sign-in");
    expect(html).toContain("The sign-in link is invalid.");
    expect(html).toContain("Try again");
    expect(html).not.toContain("Sign in with wallet");
  });

  it("keeps the polling flow on the request loader", async () => {
    const polling = await firstPaint(`/auth/requests/${REQUEST_ID}?loginMethod=metamask`);
    expect(polling.html).toContain("Loading the sign-in request");
    const malformed = await firstPaint("/auth/requests/nope");
    expect(malformed.html).toContain("This sign-in link isn");
  });
});

function ready(method: AllowedMethod, params: unknown[]): ReadyRequest {
  return {
    expiration: new Date(Date.now() + 120_000).toISOString(),
    code: 7,
    method,
    params,
    sender: SENDER,
    challenge: "c",
  };
}

type CardOverrides = Partial<Parameters<typeof ApprovalCard>[0]>;

// The ready view is rendered the way the effect would hand it over: the recovery's verdict on the
// request decides whether the effects gate shows, and the props carry what the user has ticked.
function card(request: ReadyRequest, overrides: CardOverrides = {}): string {
  return renderToString(
    <ApprovalCard
      host="catalyst.example.com"
      request={request}
      remaining={90_000}
      mustValidate={false}
      acknowledged={false}
      onAcknowledged={() => {}}
      unverifiable={unverifiableReason(request.method, request.params)}
      effectsAcknowledged={false}
      onEffectsAcknowledged={() => {}}
      error={null}
      isSigning={false}
      walletDetected
      onApprove={() => {}}
      onDeny={() => {}}
      {...overrides}
    />,
  );
}

function approveDisabled(html: string): boolean {
  const tag = html.match(/<button[^>]*>Approve in wallet<\/button>/)?.[0];
  if (!tag) throw new Error("approve button not rendered");
  return /\sdisabled(?:=""|\s|>)/.test(tag);
}

describe("approval gates", () => {
  it("needs no acknowledgment for a readable personal_sign message", () => {
    const html = card(ready("personal_sign", ["Sign in to Decentraland\nNonce: 1234", SENDER]));
    expect(html).toContain("Sign in to Decentraland");
    expect(html).not.toContain(EFFECTS_LABEL);
    expect(approveDisabled(html)).toBe(false);
  });

  it("shows the decoded text of a hex-encoded readable message", () => {
    const hex = `0x${Buffer.from("Welcome to Decentraland, please sign in").toString("hex")}`;
    const html = card(ready("personal_sign", [hex, SENDER]));
    expect(html).toContain("Welcome to Decentraland, please sign in");
    expect(html).not.toContain(EFFECTS_LABEL);
  });

  it("gates an opaque personal_sign message behind the effects acknowledgment", () => {
    const html = card(ready("personal_sign", [`0x${"ab".repeat(32)}`, SENDER]));
    expect(html).toContain("doesn&#x27;t look like readable text");
    expect(html).toContain(EFFECTS_LABEL);
    expect(approveDisabled(html)).toBe(true);
  });

  it("gates every typed-data request and keeps Approve disabled until acknowledged", () => {
    const request = ready("eth_signTypedData_v4", [SENDER, PERMIT]);
    const unchecked = card(request);
    expect(unchecked).toContain("can&#x27;t preview what this signature authorizes");
    expect(unchecked).toContain("effects of this signature");
    expect(approveDisabled(unchecked)).toBe(true);

    const checked = card(request, { effectsAcknowledged: true });
    expect(approveDisabled(checked)).toBe(false);
  });

  it("gates a transaction the same way, worded for a transaction", () => {
    const request = ready("eth_sendTransaction", [
      { to: "0xfef5c99885c3036e591b6e6db52482891834a5f4", data: "0xa9059cbb" },
    ]);
    const unchecked = card(request);
    expect(unchecked).toContain("effects of this transaction");
    expect(approveDisabled(unchecked)).toBe(true);
    expect(approveDisabled(card(request, { effectsAcknowledged: true }))).toBe(false);
  });

  it("keeps the code-match gate independent of the effects gate", () => {
    const request = ready("eth_signTypedData_v3", [SENDER, PERMIT]);
    const effectsOnly = card(request, { mustValidate: true, effectsAcknowledged: true });
    expect(effectsOnly).toContain("matches the one shown on my device");
    expect(approveDisabled(effectsOnly)).toBe(true);
    const codeOnly = card(request, { mustValidate: true, acknowledged: true });
    expect(approveDisabled(codeOnly)).toBe(true);
    const both = card(request, { mustValidate: true, acknowledged: true, effectsAcknowledged: true });
    expect(approveDisabled(both)).toBe(false);
  });
});

describe("transaction preview", () => {
  it("previews only to, data and value and names the fields the wallet never receives", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0xa9059cbb", value: "0x1", gas: "0x5208", nonce: "0x1" },
    ]);
    const summary = describeRequest(request);
    expect(JSON.parse(summary.detail)).toEqual({ to: TO, data: "0xa9059cbb", value: "0x1" });
    expect(summary.note).toBe("Not sent to the wallet: gas, nonce");

    const html = card(request);
    expect(html).toContain("Not sent to the wallet: gas, nonce");
    expect(html).not.toContain("0x5208");
  });

  it("previews exactly what is dispatched, minus the connected account", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0x", gas: "0x5208", from: "0x000000000000000000000000000000000000dead" },
    ]);
    const dispatched = buildWalletRequest(request, SENDER);
    if (!dispatched.ok) throw new Error(dispatched.rejection.message);
    const { from, ...reviewed } = dispatched.request.params[0] as Record<string, unknown>;
    expect(from).toBe(SENDER);
    expect(JSON.parse(describeRequest(request).detail)).toEqual(reviewed);
    expect(describeRequest(request).note).toBe("Not sent to the wallet: gas, from");
  });

  it("previews a plain transaction without the note", () => {
    const request = ready("eth_sendTransaction", [{ to: TO, data: "0xa9059cbb", value: "0x0" }]);
    expect(describeRequest(request).note).toBeUndefined();
    const html = card(request);
    expect(html).toContain(TO);
    expect(html).not.toContain("Not sent to the wallet");
  });
});
