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

describe("loader and first paint", () => {
  const configured = process.env.AUTH_API_URL;
  afterEach(() => {
    if (configured === undefined) delete process.env.AUTH_API_URL;
    else process.env.AUTH_API_URL = configured;
  });

  it("parses the deep-link flags, the auth-api target and the explicit request id", async () => {
    process.env.AUTH_API_URL = "https://auth.example.com/";
    const configured = await firstPaint(`/auth/requests/${REQUEST_ID}?flow=deeplink`);
    expect(configured.data.authApiUrl).toBe("https://auth.example.com");

    delete process.env.AUTH_API_URL;
    const { data, html } = await firstPaint(
      `/auth/requests/${REQUEST_ID}?loginMethod=Metamask&flow=deeplink&bridgeOnly`,
    );
    expect(data.isDeepLink).toBe(true);
    expect(data.deepLinkIdValid).toBe(true);
    expect(data.bridgeOnly).toBe(true);
    expect(data.authRequestId).toBeNull();
    expect(data.loginMethod).toBe("Metamask");
    expect(data.authApiUrl).toBe("https://auth-api.catalyst.example.com");
    expect(html).toContain("Sign in to Decentraland");
    expect(html).toContain("Sign in with wallet");
    expect(html).not.toContain("Loading the sign-in request");

    const explicit = await firstPaint(`/auth/requests/${REQUEST_ID}?flow=deeplink&authRequestId=req-7`);
    expect(explicit.data.authRequestId).toBe("req-7");
  });

  it("rejects a malformed deep-link id and keeps the polling flow on the request loader", async () => {
    const malformed = await firstPaint("/auth/requests/not-a-uuid?flow=deeplink");
    expect(malformed.data.isDeepLink).toBe(true);
    expect(malformed.data.deepLinkIdValid).toBe(false);
    expect(malformed.html).toContain("The sign-in link is invalid.");
    expect(malformed.html).toContain("Try again");
    expect(malformed.html).not.toContain("Sign in with wallet");

    const polling = await firstPaint(`/auth/requests/${REQUEST_ID}?loginMethod=metamask`);
    expect(polling.data.isDeepLink).toBe(false);
    expect(polling.data.bridgeOnly).toBe(false);
    expect(polling.data.valid).toBe(true);
    expect(polling.data.loginMethod).toBe("metamask");
    expect(polling.html).toContain("Loading the sign-in request");
    expect((await firstPaint("/auth/requests/nope")).html).toContain("This sign-in link isn");
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

const EVERY_KIND: [string, AllowedMethod, unknown[]][] = [
  ["a message", "personal_sign", ["Sign in to Decentraland\nNonce: 1234", SENDER]],
  ["typed data", "eth_signTypedData_v4", [SENDER, PERMIT]],
  ["a transaction", "eth_sendTransaction", [{ to: TO, data: "0xa9059cbb" }]],
];

describe("approval gates", () => {
  it("gates a personal_sign message, readable or opaque, behind the effects acknowledgment", () => {
    const readable = ready("personal_sign", ["Sign in to Decentraland\nNonce: 1234", SENDER]);
    const html = card(readable);
    expect(html).toContain("Sign in to Decentraland");
    expect(html).toContain("log you in to another site");
    expect(html).not.toContain("doesn&#x27;t look like readable text");
    expect(html).toContain(EFFECTS_LABEL);
    expect(approveDisabled(html)).toBe(true);
    expect(approveDisabled(card(readable, { effectsAcknowledged: true }))).toBe(false);

    const hex = `0x${Buffer.from("Welcome to Decentraland, please sign in").toString("hex")}`;
    expect(card(ready("personal_sign", [hex, SENDER]))).toContain("Welcome to Decentraland, please sign in");

    const opaque = card(ready("personal_sign", [`0x${"ab".repeat(32)}`, SENDER]));
    expect(opaque).toContain("doesn&#x27;t look like readable text");
    expect(opaque).not.toContain("log you in to another site");
    expect(opaque).toContain(EFFECTS_LABEL);
    expect(approveDisabled(opaque)).toBe(true);
  });

  it("shows a bidi override in a message, a sender or a non-text dump as a visible escape", () => {
    const message = ready("personal_sign", ["Pay \u{202e}0xattacker\u{202c} now", SENDER]);
    expect(describeRequest(message).detail).toContain("u{202e}");
    expect(describeRequest(message).detail).not.toContain("\u{202e}");
    expect(card(message)).not.toContain("\u{202e}");

    const sender = card({ ...ready("personal_sign", ["hello", SENDER]), sender: "0x\u{202e}dead" });
    expect(sender).not.toContain("\u{202e}");
    expect(sender).toContain("u{202e}");

    const longSender = card({
      ...ready("personal_sign", ["hello", SENDER]),
      sender: `0x\u{202e}${"ab".repeat(20)}`,
    });
    expect(longSender).not.toContain("\u{202e}");
    expect(longSender).toContain("0x\\u{202e}");

    const dump = describeRequest(ready("personal_sign", [{ note: "ok\u{202e}drawkcab" }, SENDER])).detail;
    expect(dump).toContain("\n");
    expect(dump).toContain("u{202e}");
    expect(dump).not.toContain("\u{202e}");
    expect(JSON.parse(describeRequest(ready("personal_sign", [{ note: "hi" }, SENDER])).detail)).toEqual([
      { note: "hi" },
      SENDER,
    ]);
  });

  it("gates typed data and transactions and keeps Approve disabled until acknowledged", () => {
    const typed = ready("eth_signTypedData_v4", [SENDER, PERMIT]);
    const unchecked = card(typed);
    expect(unchecked).toContain("can&#x27;t preview what this signature authorizes");
    expect(unchecked).toContain(EFFECTS_LABEL);
    expect(approveDisabled(unchecked)).toBe(true);
    expect(approveDisabled(card(typed, { effectsAcknowledged: true }))).toBe(false);

    const transaction = ready("eth_sendTransaction", [{ to: TO, data: "0xa9059cbb" }]);
    expect(card(transaction)).toContain(EFFECTS_LABEL);
    expect(approveDisabled(card(transaction))).toBe(true);
    expect(approveDisabled(card(transaction, { effectsAcknowledged: true }))).toBe(false);
  });

  it("escapes and bounds a typed-data payload without changing how it was signed", () => {
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

    expect(describeRequest(ready("eth_signTypedData_v4", [SENDER, "not json \u{202e}0xattacker"])).detail).toBe(
      "not json \\u{202e}0xattacker",
    );
    expect(
      describeRequest(
        ready("eth_signTypedData_v4", [SENDER, "Order\n\ttoken: MANA\n\tto: \u{202e}0xattacker"]),
      ).detail,
    ).toBe("Order\n\ttoken: MANA\n\tto: \\u{202e}0xattacker");

    const oversized = describeRequest(
      ready("eth_signTypedData_v4", [
        SENDER,
        JSON.stringify({
          domain: { name: "Token" },
          primaryType: "Permit",
          types: { Permit: [{ name: "memo", type: "string" }] },
          message: { memo: "\u{202e}".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS / 4) },
        }),
      ]),
    ).detail;
    expect(oversized.length).toBeLessThan(MAX_DISPLAYED_TYPED_DATA_CHARS + 200);
    expect(oversized).toContain("your wallet shows what it signs");
    expect(oversized).not.toContain("\u{202e}");
  });

  it("shows every dApp-signed typed-data fixture exactly as its dApp signed it", () => {
    expect(Object.keys(SIGNED_BY_DAPPS).length).toBeGreaterThan(0);
    for (const [label, typedData] of Object.entries(SIGNED_BY_DAPPS)) {
      const request = ready("eth_signTypedData_v4", [SENDER, JSON.stringify(typedData)]);
      expect.soft(JSON.parse(describeRequest(request).detail), label).toEqual(typedData);
    }
  });

  it("paints the acknowledgment of every kind open and its payload reachable before measuring", () => {
    for (const [label, method, params] of EVERY_KIND) {
      const html = card(ready(method, params));
      const checkbox = html.match(/<input[^>]*type="checkbox"[^>]*>/g)?.at(-1);
      expect(checkbox, label).toBeDefined();
      expect(/\sdisabled/.test(checkbox!), label).toBe(false);
      expect(html, label).not.toContain("Scroll to the end of the content");
      const block = html.match(/<pre[^>]*>/)?.[0] ?? "";
      expect(block, label).toContain('tabindex="0"');
      expect(block, label).toContain('aria-labelledby="auth-payload-label"');
      expect(html, label).toContain('id="auth-payload-label"');
    }
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
  it("previews to, data and value and names or counts the fields the wallet never receives", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0xa9059cbb", value: "0x1", gas: "0x5208", nonce: "0x1" },
    ]);
    const summary = describeRequest(request);
    expect(JSON.parse(summary.detail)).toEqual({ to: TO, data: "0xa9059cbb", value: "0x1" });
    expect(summary.note).toBe("Not sent to the wallet: gas, nonce");
    const html = card(request);
    expect(html).toContain("Not sent to the wallet: gas, nonce");
    expect(html).not.toContain("0x5208");

    expect(
      describeRequest(
        ready("eth_sendTransaction", [
          { to: TO, data: "0x", gas: "0x1", nonce: "0x1", from: SENDER, type: "0x2", chainId: "0x89" },
        ]),
      ).note,
    ).toBe("Not sent to the wallet: gas, nonce, from and 2 more");

    const plain = ready("eth_sendTransaction", [{ to: TO, data: "0xa9059cbb", value: "0x0" }]);
    expect(describeRequest(plain).note).toBeUndefined();
    expect(card(plain)).toContain(TO);
    expect(card(plain)).not.toContain("Not sent to the wallet");
  });

  it("escapes and bounds the dropped field names it lists", () => {
    const request = ready("eth_sendTransaction", [
      { to: TO, data: "0xa9059cbb", "gas\u{202e}": "0x1", "nonce\u{200b}": 1 },
    ]);
    const { note } = describeRequest(request);
    expect(note).toContain("u{202e}");
    expect(note).toContain("u{200b}");
    expect(note).not.toContain("\u{202e}");
    expect(note).not.toContain("\u{200b}");
    expect(card(request)).not.toContain("\u{202e}");

    const huge = describeRequest(
      ready("eth_sendTransaction", [{ to: TO, data: "0x", [`gas${"a".repeat(600 * 1024)}`]: "0x1" }]),
    ).note ?? "";
    expect(huge.length).toBeLessThan("Not sent to the wallet: ".length + 64);
    expect(huge).toContain("\u{2026}");
    expect(huge).not.toContain("truncated");
  });

  it("previews exactly what is dispatched: the connected account, hex quantities and lowercase targets", () => {
    const withFrom = ready("eth_sendTransaction", [
      { to: TO, data: "0x", gas: "0x5208", from: "0x000000000000000000000000000000000000dead" },
    ]);
    const dispatched = buildWalletRequest(withFrom, SENDER);
    if (!dispatched.ok) throw new Error(dispatched.rejection.message);
    const { from, ...reviewed } = dispatched.request.params[0] as Record<string, unknown>;
    expect(from).toBe(SENDER);
    expect(JSON.parse(describeRequest(withFrom).detail)).toEqual(reviewed);
    expect(describeRequest(withFrom).note).toBe("Not sent to the wallet: gas, from");

    const decimal = ready("eth_sendTransaction", [{ to: TO, data: "0x", value: "10000000" }]);
    expect(JSON.parse(describeRequest(decimal).detail)).toEqual({ to: TO, data: "0x", value: "0x989680" });
    const decimalDispatched = buildWalletRequest(decimal, SENDER);
    if (!decimalDispatched.ok) throw new Error(decimalDispatched.rejection.message);
    expect((decimalDispatched.request.params[0] as Record<string, unknown>).value).toBe("0x989680");
    expect(card(decimal)).toContain("0x989680");
    expect(card(decimal)).not.toContain("10000000");

    const shouty = ready("eth_sendTransaction", [
      { to: "0XFEF5C99885C3036E591B6E6DB52482891834A5F4", data: "0xA9059CBB" },
    ]);
    expect(JSON.parse(describeRequest(shouty).detail)).toEqual({ to: TO, data: "0xa9059cbb", value: "0x0" });
    expect(card(shouty)).toContain(TO);
  });
});

describe("what a malicious request of this kind could do", () => {
  it("warns per request kind and names the wallet operation only for signatures", () => {
    const transaction = card(ready("eth_sendTransaction", [{ to: TO, data: "0xa9059cbb" }]));
    expect(transaction).toContain("If this request is malicious, it could:");
    expect(transaction).toContain("Move, sell or destroy any tokens, NFTs, LAND or names your wallet holds.");
    expect(transaction).toContain("Once sent, it can");
    expect(transaction).not.toContain("a signature doesn");
    expect(transaction).not.toContain("Wallet method");

    const typed = card(ready("eth_signTypedData_v4", [SENDER, PERMIT]));
    expect(typed).toContain("Authorize an order, a listing or a spending permission over your assets.");
    expect(typed).toContain("anyone who holds it can submit it");
    expect(typed).not.toContain("Move, sell or destroy");
    expect(typed).toContain("eth_signTypedData_v4");
    expect(card(ready("eth_signTypedData_v3", [SENDER, PERMIT]))).toContain("eth_signTypedData_v3");

    const message = card(ready("personal_sign", ["Please confirm your order", SENDER]));
    expect(message).toContain("Log you in to another site or app as you.");
    expect(message).toContain("Only continue if you trust the scene or app that asked for this.");
    expect(message).not.toContain("Move, sell or destroy");
    expect(message).toContain("personal_sign");
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

  it("puts a deadline on reads, none on the outcome write, and copes without AbortSignal.timeout", async () => {
    const reads = captureFetch(async () => jsonBody({}));
    await loadRequest(REQUEST_ID);
    await validationRequirement(REQUEST_ID);
    expect(reads).toHaveLength(2);
    for (const init of reads) {
      expect(init.signal).toBeInstanceOf(AbortSignal);
      expect(init.signal?.aborted).toBe(false);
    }

    const writes = captureFetch(async () => jsonBody({}));
    await postOutcome(REQUEST_ID, { sender: SENDER, result: "0x" });
    expect(writes).toHaveLength(1);
    expect(writes[0]?.signal).toBeUndefined();

    vi.stubGlobal("AbortSignal", class {});
    const legacy = captureFetch(async () =>
      jsonBody({ expiration: new Date(Date.now() + 120_000).toISOString(), code: 7, method: "personal_sign" }),
    );
    expect((await loadRequest(REQUEST_ID)).kind).toBe("ok");
    expect(legacy[0]?.signal).toBeUndefined();
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

  it("gives a hung request read up and treats a validation lookup that cannot answer as no verdict", async () => {
    captureFetch(async () => {
      throw timedOut();
    });
    expect(await loadRequest(REQUEST_ID)).toEqual({
      kind: "error",
      message: "Couldn't reach the sign-in server.",
    });
    expect(await validationRequirement(REQUEST_ID)).toBeNull();

    captureFetch(async () => new Response("no", { status: 503 }));
    expect(await validationRequirement(REQUEST_ID)).toBeNull();

    captureFetch(async () => jsonBody({ requiresValidation: true }));
    expect(await validationRequirement(REQUEST_ID)).toBe(true);
    captureFetch(async () => jsonBody({ requiresValidation: false }));
    expect(await validationRequirement(REQUEST_ID)).toBe(false);
  });

  it("keeps the device-code gate unless the sign-in server definitely says no", async () => {
    captureFetch(async () => {
      throw timedOut();
    });
    expect(await requiresValidation(REQUEST_ID)).toBe(true);
    captureFetch(async () => jsonBody({ requiresValidation: true }));
    expect(await requiresValidation(REQUEST_ID)).toBe(true);
    captureFetch(async () => jsonBody({ requiresValidation: false }));
    expect(await requiresValidation(REQUEST_ID)).toBe(false);

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

  it("carries the deadline into the deep-link handoff and keeps the identity for a retry", async () => {
    let seen: AbortSignal | undefined;
    const result = await handoffWithDeadline(async (opts) => {
      seen = opts.signal;
      return { identityId: "9b2c1a1e-4c3d-4f5e-8a6b-7c8d9e0f1a2b" };
    });
    expect(seen).toBeInstanceOf(AbortSignal);
    expect(result.identityId).toBe("9b2c1a1e-4c3d-4f5e-8a6b-7c8d9e0f1a2b");

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

    await expect(
      handoffWithDeadline(async () => {
        throw new Error("Request sender does not match identity owner");
      }),
    ).rejects.toThrow("Request sender does not match identity owner");
  });

  it("never lets a report the page only records hold the screen or throw at the caller", async () => {
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

    captureFetch(async () => {
      throw new TypeError("Failed to fetch");
    });
    expect(await reportOutcome(REQUEST_ID, { sender: SENDER, error: { code: 999, message: "boom" } })).toBeNull();
    captureFetch(async () => new Response("no", { status: 500 }));
    expect(await reportOutcome(REQUEST_ID, { sender: SENDER, error: { code: 999, message: "boom" } })).toBeNull();
  });
});
