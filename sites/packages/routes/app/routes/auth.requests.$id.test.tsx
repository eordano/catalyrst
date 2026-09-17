import { renderToString } from "react-dom/server";
import {
  createStaticHandler,
  createStaticRouter,
  StaticRouterProvider,
  type LoaderFunction,
} from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { AuthIdentity } from "@data/lib/auth/types";

import {
  AUTH_API_TIMEOUT_MS,
  handoffWithDeadline,
  loadRequest,
  postOutcome,
  reportOutcome,
  requiresValidation,
  validationRequirement,
  withDeadline,
} from "../lib/auth-api-client";
import { completeDeepLinkSignIn } from "../lib/auth-deeplink";
import {
  buildWalletRequest,
  unverifiableReason,
  type AllowedMethod,
} from "../lib/auth-request-params";
import type { ReadyRequest } from "../lib/auth-request-recovery";
import { MAX_DISPLAYED_TYPED_DATA_CHARS } from "../lib/auth-typed-data-escape";
import { SIGNED_BY_DAPPS } from "../lib/auth-typed-data-fixtures";

import AuthRequestRoute, {
  ApprovalCard,
  describeRequest,
  loader,
} from "./auth.requests.$id";

const REQUEST_ID = "123e4567-e89b-42d3-a456-426614174000";
const SENDER = "0x1234567890abcdef1234567890abcdef12345678";
const TO = "0xfef5c99885c3036e591b6e6db52482891834a5f4";
const PERMIT = JSON.stringify({
  domain: { name: "Token", version: "1", chainId: 137 },
  primaryType: "Permit",
  types: { Permit: [{ name: "spender", type: "address" }] },
  message: { spender: "0x000000000000000000000000000000000000dead" },
});
const EFFECTS_LABEL = "I take full responsibility for what this action does";
const CODE_LABEL = "I confirm the code above matches the one shown on my device.";
const IDENTITY: AuthIdentity = {
  signer: SENDER,
  ephemeral: { address: TO, privateKey: "0x01" },
  expiration: new Date(Date.now() + 600_000).toISOString(),
  authChain: [],
};

const routes = [
  {
    path: "/auth/requests/:id",
    Component: AuthRequestRoute,
    loader: loader as unknown as LoaderFunction,
  },
];

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

  it("shows an override inside a signed message as a visible escape", () => {
    const request = ready("personal_sign", ["Pay \u{202e}0xattacker\u{202c} now", SENDER]);
    const { detail } = describeRequest(request);
    expect(detail).toContain("u{202e}");
    expect(detail).not.toContain("\u{202e}");
    expect(card(request)).not.toContain("\u{202e}");
  });

  it("shows a sender that would reorder its chip as a visible escape", () => {
    const request = { ...ready("personal_sign", ["hello", SENDER]), sender: "0x\u{202e}dead" };
    const html = card(request);
    expect(html).not.toContain("\u{202e}");
    expect(html).toContain("u{202e}");
  });

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
    expect(unchecked).toContain(EFFECTS_LABEL);
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

  it("keeps the lines of an unparsable typed-data param laid out as they were signed", () => {
    const { detail } = describeRequest(
      ready("eth_signTypedData_v4", [SENDER, "Order\n\ttoken: MANA\n\tto: \u{202e}0xattacker"]),
    );
    expect(detail).toBe("Order\n\ttoken: MANA\n\tto: \\u{202e}0xattacker");
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
    expect(unchecked).toContain(EFFECTS_LABEL);
    expect(approveDisabled(unchecked)).toBe(true);
    expect(approveDisabled(card(request, { effectsAcknowledged: true }))).toBe(false);
  });

  it.each([
    ["a message", "personal_sign", ["Sign in to Decentraland\nNonce: 1234", SENDER]],
    ["typed data", "eth_signTypedData_v4", [SENDER, PERMIT]],
    ["a transaction", "eth_sendTransaction", [{ to: TO, data: "0xa9059cbb" }]],
  ] as [string, AllowedMethod, unknown[]][])(
    "paints the acknowledgment of %s open before its payload block has been measured",
    (_label, method, params) => {
      const html = card(ready(method, params));
      const checkbox = html.match(/<input[^>]*type="checkbox"[^>]*>/g)?.at(-1);
      expect(checkbox).toBeDefined();
      expect(/\sdisabled/.test(checkbox!)).toBe(false);
      expect(html).not.toContain("Scroll to the end of the content");
    },
  );

  it.each([
    ["a message", "personal_sign", ["Sign in to Decentraland\nNonce: 1234", SENDER]],
    ["typed data", "eth_signTypedData_v4", [SENDER, PERMIT]],
    ["a transaction", "eth_sendTransaction", [{ to: TO, data: "0xa9059cbb" }]],
  ] as [string, AllowedMethod, unknown[]][])(
    "gives the payload block of %s a name and a way to reach it without a pointer",
    (_label, method, params) => {
      const html = card(ready(method, params));
      const block = html.match(/<pre[^>]*>/)?.[0];
      expect(block).toBeDefined();
      expect(block).toContain('role="region"');
      expect(block).toContain('tabindex="0"');
      expect(block).toContain('aria-labelledby="auth-payload-label"');
      expect(html).toContain('id="auth-payload-label"');
    },
  );

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

  it("names three dropped fields and counts the rest", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0x", gas: "0x1", nonce: "0x1", from: SENDER, type: "0x2", chainId: "0x89" },
    ]);
    expect(describeRequest(request).note).toBe(
      "Not sent to the wallet: gas, nonce, from and 2 more",
    );
  });

  it("bounds every field name it lists, not just the note as a whole", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0x", [`gas${"a".repeat(600 * 1024)}`]: "0x1" },
    ]);
    const note = describeRequest(request).note ?? "";
    expect(note.length).toBeLessThan("Not sent to the wallet: ".length + 64);
    expect(note).toContain("\u{2026}");
    expect(note).not.toContain("truncated");
  });
});

describe("what a malicious request of this kind could do", () => {
  it("warns a transaction moves assets and cannot be undone", () => {
    const html = card(ready("eth_sendTransaction", [{ to: TO, data: "0xa9059cbb" }]));
    expect(html).toContain("If this request is malicious, it could:");
    expect(html).toContain("Move, sell or destroy any tokens, NFTs, LAND or names your wallet holds.");
    expect(html).toContain("Once sent, it can");
    expect(html).not.toContain("a signature doesn");
  });

  it("warns a typed-data signature is a bearer authorization", () => {
    const html = card(ready("eth_signTypedData_v4", [SENDER, PERMIT]));
    expect(html).toContain("Authorize an order, a listing or a spending permission over your assets.");
    expect(html).toContain("anyone who holds it can submit it");
    expect(html).not.toContain("Move, sell or destroy");
  });

  it("warns a signed message can be a login or an off-chain order", () => {
    const html = card(ready("personal_sign", ["Please confirm your order", SENDER]));
    expect(html).toContain("Log you in to another site or app as you.");
    expect(html).toContain("Only continue if you trust the scene or app that asked for this.");
    expect(html).not.toContain("Move, sell or destroy");
  });

  it("says which wallet operation a signature is consenting to, and omits it for a transaction", () => {
    expect(card(ready("eth_signTypedData_v3", [SENDER, PERMIT]))).toContain("eth_signTypedData_v3");
    expect(card(ready("eth_signTypedData_v4", [SENDER, PERMIT]))).toContain("eth_signTypedData_v4");
    expect(card(ready("personal_sign", ["hello", SENDER]))).toContain("personal_sign");
    const transaction = card(ready("eth_sendTransaction", [{ to: TO, data: "0x" }]));
    expect(transaction).not.toContain("Wallet method");
  });
});

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

describe("every auth-api read the review waits on is bounded", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function jsonBody(body: unknown, status = 200): Response {
    return new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    });
  }

  function captureFetch(respond: () => Promise<Response>): RequestInit[] {
    const calls: RequestInit[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (_input: string, init: RequestInit = {}) => {
        calls.push(init);
        return respond();
      }),
    );
    return calls;
  }

  function timedOut(): DOMException {
    return new DOMException("The operation was aborted due to timeout", "TimeoutError");
  }

  it("carries the same deadline on the request and validation reads", async () => {
    const calls = captureFetch(async () => jsonBody({}));
    await loadRequest(REQUEST_ID);
    await validationRequirement(REQUEST_ID);

    expect(AUTH_API_TIMEOUT_MS).toBe(10_000);
    expect(calls).toHaveLength(2);
    for (const init of calls) {
      expect(init.signal).toBeInstanceOf(AbortSignal);
      expect(init.signal?.aborted).toBe(false);
    }
  });

  it("never abandons the outcome of an executed wallet interaction", async () => {
    const calls = captureFetch(async () => jsonBody({}));
    await postOutcome(REQUEST_ID, { sender: SENDER, result: "0x" });

    expect(calls).toHaveLength(1);
    expect(calls[0]?.signal).toBeUndefined();
  });

  it("leaves the loading screen at the deadline while the outcome write stays in flight", async () => {
    let delivered = false;
    const write = new Promise<{ ok: boolean }>((resolve) => {
      setTimeout(() => {
        delivered = true;
        resolve({ ok: true });
      }, 20);
    });

    expect(await withDeadline(write, 1, { ok: false })).toEqual({ ok: false });
    expect(delivered).toBe(false);
    expect(await write).toEqual({ ok: true });
    expect(delivered).toBe(true);
  });

  it("reads the request where the browser has no AbortSignal.timeout instead of failing every sign-in", async () => {
    vi.stubGlobal("AbortSignal", class {});
    const calls = captureFetch(async () =>
      jsonBody({
        expiration: new Date(Date.now() + 120_000).toISOString(),
        code: 7,
        method: "personal_sign",
      }),
    );

    expect((await loadRequest(REQUEST_ID)).kind).toBe("ok");
    expect(calls[0]?.signal).toBeUndefined();
  });

  it("gives a hung request read up rather than holding the page with nothing to deny", async () => {
    captureFetch(async () => {
      throw timedOut();
    });
    expect(await loadRequest(REQUEST_ID)).toEqual({
      kind: "error",
      message: "Couldn't reach the sign-in server.",
    });
  });

  it("treats a validation lookup that cannot answer as no verdict", async () => {
    captureFetch(async () => {
      throw timedOut();
    });
    expect(await validationRequirement(REQUEST_ID)).toBeNull();

    captureFetch(async () => new Response("no", { status: 503 }));
    expect(await validationRequirement(REQUEST_ID)).toBeNull();
  });

  it("reads a definite validation answer either way", async () => {
    captureFetch(async () => jsonBody({ requiresValidation: true }));
    expect(await validationRequirement(REQUEST_ID)).toBe(true);

    captureFetch(async () => jsonBody({ requiresValidation: false }));
    expect(await validationRequirement(REQUEST_ID)).toBe(false);
  });

  it("keeps the device-code gate on the card when the validation read cannot answer", async () => {
    captureFetch(async () => new Response("no", { status: 503 }));
    const mustValidate = await requiresValidation(REQUEST_ID);
    expect(mustValidate).toBe(true);

    const request = ready("eth_signTypedData_v4", [SENDER, PERMIT]);
    const gated = card(request, { mustValidate, effectsAcknowledged: true });
    expect(gated).toContain(CODE_LABEL);
    expect(approveDisabled(gated)).toBe(true);
    const ticked = card(request, { mustValidate, effectsAcknowledged: true, acknowledged: true });
    expect(approveDisabled(ticked)).toBe(false);
  });

  it("keeps the device-code gate when the validation read times out and drops it only on a server no", async () => {
    captureFetch(async () => {
      throw timedOut();
    });
    expect(await requiresValidation(REQUEST_ID)).toBe(true);

    captureFetch(async () => jsonBody({ requiresValidation: true }));
    expect(await requiresValidation(REQUEST_ID)).toBe(true);

    captureFetch(async () => jsonBody({ requiresValidation: false }));
    expect(await requiresValidation(REQUEST_ID)).toBe(false);
  });

  it("carries the deadline into the deep-link handoff", async () => {
    let seen: AbortSignal | undefined;
    const result = await handoffWithDeadline(async (opts) => {
      seen = opts.signal;
      return { identityId: "9b2c1a1e-4c3d-4f5e-8a6b-7c8d9e0f1a2b" };
    });

    expect(seen).toBeInstanceOf(AbortSignal);
    expect(result.identityId).toBe("9b2c1a1e-4c3d-4f5e-8a6b-7c8d9e0f1a2b");
  });

  it("puts page copy on a handoff that ran out of time and keeps the identity for the retry", async () => {
    const outcome = await completeDeepLinkSignIn({
      connect: async () => SENDER,
      cachedIdentity: () => IDENTITY,
      createIdentity: async () => IDENTITY,
      postIdentity: () =>
        handoffWithDeadline(async () => {
          throw timedOut();
        }),
      isUserRejection: () => false,
    });

    expect(outcome).toEqual({
      kind: "post_error",
      message: "Couldn't reach the sign-in server.",
      identity: IDENTITY,
    });
  });

  it("passes a handoff refusal through in the sign-in server's own words", async () => {
    await expect(
      handoffWithDeadline(async () => {
        throw new Error("Request sender does not match identity owner");
      }),
    ).rejects.toThrow("Request sender does not match identity owner");
  });

  it("stops waiting on a report the page only records so the screen can move on", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      const calls = captureFetch(() => new Promise<Response>(() => {}));
      const reported = reportOutcome(REQUEST_ID, {
        sender: SENDER,
        error: { code: 4001, message: "Request rejected" },
      });
      await vi.advanceTimersByTimeAsync(AUTH_API_TIMEOUT_MS);

      expect(await reported).toBeNull();
      expect(calls).toHaveLength(1);
      expect(calls[0]?.signal).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it("records a report that the sign-in server refuses without throwing at the caller", async () => {
    captureFetch(async () => {
      throw new TypeError("Failed to fetch");
    });
    expect(
      await reportOutcome(REQUEST_ID, { sender: SENDER, error: { code: 999, message: "boom" } }),
    ).toBeNull();

    captureFetch(async () => new Response("no", { status: 500 }));
    expect(
      await reportOutcome(REQUEST_ID, { sender: SENDER, error: { code: 999, message: "boom" } }),
    ).toBeNull();
  });
});
