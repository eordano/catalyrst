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
import { MAX_DISPLAYED_TYPED_DATA_CHARS } from "../lib/auth-typed-data-escape";
import { SIGNED_BY_DAPPS } from "../lib/auth-typed-data-fixtures";

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
  // Tightened in auth #489: readability is not a verdict on what a signature is then used for, so
  // every personal_sign reaches the wallet behind the same acknowledgment an opaque one does.
  it("gates a readable personal_sign message behind the effects acknowledgment", () => {
    const request = ready("personal_sign", ["Sign in to Decentraland\nNonce: 1234", SENDER]);
    const html = card(request);
    expect(html).toContain("Sign in to Decentraland");
    expect(html).toContain("log you in to another site");
    expect(html).toContain(EFFECTS_LABEL);
    expect(approveDisabled(html)).toBe(true);
    expect(approveDisabled(card(request, { effectsAcknowledged: true }))).toBe(false);
  });

  it("shows the decoded text of a hex-encoded readable message and still gates it", () => {
    const hex = `0x${Buffer.from("Welcome to Decentraland, please sign in").toString("hex")}`;
    const html = card(ready("personal_sign", [hex, SENDER]));
    expect(html).toContain("Welcome to Decentraland, please sign in");
    expect(html).toContain(EFFECTS_LABEL);
  });

  it("wears the message warning only for a readable message, never the opaque one", () => {
    const readable = card(ready("personal_sign", ["Please confirm your order", SENDER]));
    expect(readable).toContain("log you in to another site");
    expect(readable).not.toContain("doesn&#x27;t look like readable text");

    const opaque = card(ready("personal_sign", [`0x${"ab".repeat(32)}`, SENDER]));
    expect(opaque).toContain("doesn&#x27;t look like readable text");
    expect(opaque).not.toContain("log you in to another site");
  });

  // A message with an override is already classified opaque, and an opaque one is escaped before it
  // is shown: it can neither reorder the block it sits in nor spell an address it does not sign.
  it("shows an override inside a signed message as a visible escape", () => {
    const request = ready("personal_sign", ["Pay \u{202e}0xattacker\u{202c} now", SENDER]);
    const { detail } = describeRequest(request);
    expect(detail).toContain("u{202e}");
    expect(detail).not.toContain("\u{202e}");
    expect(card(request)).not.toContain("\u{202e}");
  });

  // A sender is text the request supplied and nothing holds it to an address shape.
  it("shows a sender that would reorder its chip as a visible escape", () => {
    const request = { ...ready("personal_sign", ["hello", SENDER]), sender: "0x\u{202e}dead" };
    const html = card(request);
    expect(html).not.toContain("\u{202e}");
    expect(html).toContain("u{202e}");
  });

  // Cut first, escaped after: an escape is never shown half-written, and a hidden character that
  // survived into the kept prefix is still shown as one.
  it("escapes what survives the cut of a long sender", () => {
    const request = {
      ...ready("personal_sign", ["hello", SENDER]),
      sender: `0x\u{202e}${"ab".repeat(20)}`,
    };
    const html = card(request);
    expect(html).not.toContain("\u{202e}");
    expect(html).toContain("0x\\u{202e}");
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

  it("shows a signed string that would reorder or hide the payload as visible escapes", () => {
    const typedData = {
      domain: { name: "Decentraland\u{202e} Marketplace", version: "1\u{200b}" },
      primaryType: "Permit",
      types: { Permit: [{ name: "spender", type: "address" }, { name: "memo", type: "string" }] },
      message: {
        spender: "0x000000000000000000000000000000000000dead",
        memo: "Send 1 MANA to \u{202e}0xattacker",
      },
    };
    const request = ready("eth_signTypedData_v4", [SENDER, JSON.stringify(typedData)]);
    const { detail } = describeRequest(request);
    expect(detail).not.toContain("\u{202e}");
    expect(detail).not.toContain("\u{200b}");
    expect(detail).toContain("u{202e}");
    expect(detail).toContain("u{200b}");
    expect(card(request)).not.toContain("\u{202e}");
  });

  it("escapes an unparsable typed-data param rather than letting it act on the display", () => {
    const { detail } = describeRequest(
      ready("eth_signTypedData_v4", [SENDER, "not json \u{202e}0xattacker"]),
    );
    expect(detail).toBe("not json \\u{202e}0xattacker");
  });

  it("stops showing a payload once it is longer than the page can render", () => {
    const typedData = {
      domain: { name: "Token" },
      primaryType: "Permit",
      types: { Permit: [{ name: "memo", type: "string" }] },
      message: { memo: "\u{202e}".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS / 4) },
    };
    const { detail } = describeRequest(
      ready("eth_signTypedData_v4", [SENDER, JSON.stringify(typedData)]),
    );
    expect(detail.length).toBeLessThan(MAX_DISPLAYED_TYPED_DATA_CHARS + 200);
    expect(detail).toContain("your wallet shows what it signs");
    expect(detail).not.toContain("\u{202e}");
  });

  it.each(Object.entries(SIGNED_BY_DAPPS))(
    "shows the %s exactly as its dApp signed it",
    (_label, typedData) => {
      const request = ready("eth_signTypedData_v4", [SENDER, JSON.stringify(typedData)]);
      expect(JSON.parse(describeRequest(request).detail)).toEqual(typedData);
    },
  );

  it("gates a transaction the same way, worded for a transaction", () => {
    const request = ready("eth_sendTransaction", [
      { to: "0xfef5c99885c3036e591b6e6db52482891834a5f4", data: "0xa9059cbb" },
    ]);
    const unchecked = card(request);
    expect(unchecked).toContain("effects of this transaction");
    expect(approveDisabled(unchecked)).toBe(true);
    expect(approveDisabled(card(request, { effectsAcknowledged: true }))).toBe(false);
  });

  // SSR paints the pre-measurement state: no layout effect runs under renderToString, so this pins
  // the default-open behaviour only. Which requests the gate applies to, and what it does once the
  // block has been measured, are pinned in auth-message-scroll.test.ts.
  it("paints the acknowledgment open before the message block has been measured", () => {
    const html = card(ready("personal_sign", ["Sign in to Decentraland\nNonce: 1234", SENDER]));
    const checkbox = html.match(/<input[^>]*type="checkbox"[^>]*>/g)?.at(-1);
    expect(checkbox).toBeDefined();
    expect(/\sdisabled/.test(checkbox!)).toBe(false);
    expect(html).not.toContain("Scroll to the end of the message");
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

  it("shows a field name that would reorder the note as a visible escape", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0xa9059cbb", "gas\u{202e}": "0x1", "nonce\u{200b}": 1 },
    ]);
    const { note } = describeRequest(request);
    expect(note).toContain("u{202e}");
    expect(note).toContain("u{200b}");
    expect(note).not.toContain("\u{202e}");
    expect(note).not.toContain("\u{200b}");
    expect(card(request)).not.toContain("\u{202e}");
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

  // The dispatched value and the previewed value are one canonicalised string, so a wallet that
  // reads a decimal as text cannot sign an amount the card never showed.
  it("shows a decimal value as the hex quantity the wallet is handed", () => {
    const request = ready("eth_sendTransaction", [{ to: TO, data: "0x", value: "10000000" }]);
    const summary = describeRequest(request);
    expect(JSON.parse(summary.detail)).toEqual({ to: TO, data: "0x", value: "0x989680" });

    const dispatched = buildWalletRequest(request, SENDER);
    if (!dispatched.ok) throw new Error(dispatched.rejection.message);
    expect((dispatched.request.params[0] as Record<string, unknown>).value).toBe("0x989680");

    const html = card(request);
    expect(html).toContain("0x989680");
    expect(html).not.toContain("10000000");
  });

  it("previews a plain transaction without the note", () => {
    const request = ready("eth_sendTransaction", [{ to: TO, data: "0xa9059cbb", value: "0x0" }]);
    expect(describeRequest(request).note).toBeUndefined();
    const html = card(request);
    expect(html).toContain(TO);
    expect(html).not.toContain("Not sent to the wallet");
  });

  it("shows the target in the spelling it is dispatched in", () => {
    const request = ready("eth_sendTransaction", [
      { to: "0XFEF5C99885C3036E591B6E6DB52482891834A5F4", data: "0xA9059CBB" },
    ]);
    expect(JSON.parse(describeRequest(request).detail)).toEqual({
      to: TO,
      data: "0xa9059cbb",
      value: "0x0",
    });
    expect(card(request)).toContain(TO);
  });

  // Nothing bounds the keys a scene puts on the transaction object, so the note names a few and
  // counts the rest the way upstream's listAddresses does.
  it("names three dropped fields and counts the rest", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0x", gas: "0x1", nonce: "0x1", from: SENDER, type: "0x2", chainId: "0x89" },
    ]);
    expect(describeRequest(request).note).toBe(
      "Not sent to the wallet: gas, nonce, from and 2 more",
    );
  });

  it("truncates a note a single field name would otherwise blow up", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0x", [`gas${"a".repeat(600 * 1024)}`]: "0x1" },
    ]);
    const note = describeRequest(request).note ?? "";
    expect(note.length).toBeLessThan(MAX_DISPLAYED_TYPED_DATA_CHARS + 200);
    expect(note).toContain("truncated");
  });
});

// Unreachable from a validated request -- personal_sign refuses a non-string first param -- so this
// pins the fallback the detail box falls back to rather than a rendered class of request.
describe("a message that is not text", () => {
  it("keeps the shape of the dump it prints", () => {
    const request: ReadyRequest = ready("personal_sign", [{ note: "hi" }, SENDER]);
    const { detail } = describeRequest(request);
    expect(detail).toContain("\n");
    expect(JSON.parse(detail)).toEqual([{ note: "hi" }, SENDER]);
  });

  it("still escapes what a value carries", () => {
    const request: ReadyRequest = ready("personal_sign", [
      { note: "ok\u{202e}drawkcab" },
      SENDER,
    ]);
    const { detail } = describeRequest(request);
    expect(detail).toContain("u{202e}");
    expect(detail).not.toContain("\u{202e}");
  });
});
