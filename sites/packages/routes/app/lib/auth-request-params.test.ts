import { describe, expect, it } from "vitest";

import {
  MAX_CALLDATA_BYTES,
  MAX_SIGNATURE_PAYLOAD_CHARS,
  RPC_INVALID_PARAMS,
  RPC_METHOD_NOT_SUPPORTED,
  approveBlocked,
  buildTransactionParams,
  buildWalletRequest,
  canonicalMethod,
  decodeSignatureMessage,
  denialSender,
  isEphemeralMessage,
  isOpaqueSignatureMessage,
  isRetiredSignInMethod,
  isSameAccount,
  metaTransactionDomainProblem,
  previewTransaction,
  rejectionOutcome,
  toHexQuantity,
  unverifiableReason,
  validateAuthRequest,
  type ApprovalGates,
  type RequestRejection,
} from "./auth-request-params";
import { DAPP_USER, SIGNED_BY_DAPPS } from "./auth-typed-data-fixtures";

const SIGNER = "0x1234567890AbcdEF1234567890aBcdef12345678";
const OTHER = "0x0000000000000000000000000000000000000002";
const TO = "0xfef5c99885c3036e591b6e6db52482891834a5f4";

const STATEMENT = JSON.stringify({
  domain: { name: "Decentraland", version: "1" },
  primaryType: "Statement",
  types: { Statement: [{ name: "text", type: "string" }] },
  message: { text: "Sign in to Decentraland" },
});
const PERMIT = JSON.stringify({
  domain: { name: "Token", version: "1", chainId: 1, verifyingContract: "0x0000000000000000000000000000000000000001" },
  primaryType: "Permit",
  types: {
    Permit: [
      { name: "owner", type: "address" },
      { name: "spender", type: "address" },
      { name: "value", type: "uint256" },
      { name: "nonce", type: "uint256" },
      { name: "deadline", type: "uint256" },
    ],
  },
  message: { owner: SIGNER, spender: "0x000000000000000000000000000000000000dead", value: "1000", nonce: "0", deadline: "9999999999" },
});

const MALFORMED_SIGNATURE = { code: RPC_INVALID_PARAMS, kind: "malformed_signature" };
const MALFORMED_TRANSACTION = { code: RPC_INVALID_PARAMS, kind: "malformed_transaction" };

function rejection(method: string, params: unknown[] | undefined, signer: string | null): RequestRejection {
  const result = validateAuthRequest(method, params, signer);
  if (result.ok) throw new Error(`expected ${method} to be rejected`);
  return result.rejection;
}

const EPHEMERAL = `0x${"ab".repeat(20)}`;

function ephemeralPayload(
  overrides: {
    header?: string;
    addressPrefix?: string;
    address?: string;
    expirationPrefix?: string;
    expiration?: string;
  } = {},
): string {
  const {
    header = "Decentraland Login",
    addressPrefix = "Ephemeral address: ",
    address = EPHEMERAL,
    expirationPrefix = "Expiration: ",
    expiration = "2100-01-01T00:00:00.000Z",
  } = overrides;
  return [header, `${addressPrefix}${address}`, `${expirationPrefix}${expiration}`].join("\n");
}

function toHex(text: string): string {
  return Array.from(new TextEncoder().encode(text))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

function permitWith(message: string): string {
  return `{"primaryType":"Permit","types":{},"domain":{},"message":${message}}`;
}

describe("canonicalMethod", () => {
  it("returns supported methods in canonical casing and refuses the rest", () => {
    for (const method of ["personal_sign", "eth_signTypedData_v3", "eth_signTypedData_v4", "eth_sendTransaction"]) {
      expect(canonicalMethod(method), method).toBe(method);
    }
    expect(canonicalMethod("ETH_SENDTRANSACTION")).toBe("eth_sendTransaction");
    expect(canonicalMethod(" Personal_Sign ")).toBe("personal_sign");
    for (const method of ["eth_signTypedData", "eth_sign", "dcl_personal_sign", "wallet_addEthereumChain", ""]) {
      expect(canonicalMethod(method), method).toBeNull();
    }
  });

  it("recognizes the retired sign-in method for the outdated-client copy", () => {
    expect(isRetiredSignInMethod("dcl_personal_sign")).toBe(true);
    expect(isRetiredSignInMethod("DCL_PERSONAL_SIGN")).toBe(true);
    expect(isRetiredSignInMethod("personal_sign")).toBe(false);
  });
});

describe("validateAuthRequest method and impersonation guards", () => {
  it("reports unsupported, retired and legacy v1 methods with -32601", () => {
    expect(rejection("eth_sign", [SIGNER, "0x00"], SIGNER)).toMatchObject({
      code: RPC_METHOD_NOT_SUPPORTED,
      kind: "unsupported_method",
    });
    expect(rejection("dcl_personal_sign", ["hello", SIGNER], SIGNER)).toMatchObject({
      code: RPC_METHOD_NOT_SUPPORTED,
      kind: "retired_sign_in",
    });
    expect(rejection("eth_signTypedData", [SIGNER, PERMIT], SIGNER).code).toBe(RPC_METHOD_NOT_SUPPORTED);
  });

  it("blocks an identity payload whatever its header, line prefixes, address casing or encoding", () => {
    const payloads: [string, string][] = [
      ["the canonical payload", ephemeralPayload()],
      [
        "re-prefixed lines two and three",
        ephemeralPayload({
          addressPrefix: "x".repeat("Ephemeral address: ".length),
          expirationPrefix: "x".repeat("Expiration: ".length),
        }),
      ],
      ["a lowercase authority without 0x", ephemeralPayload({ address: "ab".repeat(20) })],
      ["an uppercase authority without 0x", ephemeralPayload({ address: "AB".repeat(20) })],
      ["any header line", ephemeralPayload({ header: "Please sign in to continue" })],
      ["an empty header line", ephemeralPayload({ header: "" })],
      ["an unrelated first line", `Anything\nEphemeral address: ${EPHEMERAL}\nExpiration: 2100-01-01T00:00:00.000Z`],
    ];
    for (const [label, payload] of payloads) {
      for (const message of [payload, `0x${toHex(payload)}`]) {
        expect(isEphemeralMessage(message), label).toBe(true);
        expect(rejection("personal_sign", [message, SIGNER], SIGNER), label).toMatchObject({
          code: RPC_INVALID_PARAMS,
          kind: "impersonated_sign_in",
        });
      }
    }
    expect(isEphemeralMessage("Decentraland Login\nnot an ephemeral payload")).toBe(false);
  });

  it("leaves a payload alone that would not parse into an identity either", () => {
    const messages: [string, string][] = [
      ["no third line", `Decentraland Login\nEphemeral address: ${EPHEMERAL}`],
      ["no address at the address offset", ephemeralPayload({ address: "0xabc" })],
      ["no parseable expiration at the expiration offset", ephemeralPayload({ expiration: "no date here at all" })],
      ["an empty expiration", ephemeralPayload({ expiration: "" })],
    ];
    for (const [label, message] of messages) {
      expect(isEphemeralMessage(message), label).toBe(false);
      expect(validateAuthRequest("personal_sign", [message, SIGNER], SIGNER).ok, label).toBe(true);
    }
  });
});

describe("validateAuthRequest signature params", () => {
  const TYPED = ["eth_signTypedData_v4", "eth_signTypedData_v3"];

  it("accepts [signer, typed data] for v3 and v4, signer casing and object payloads included", () => {
    for (const method of TYPED) {
      expect(validateAuthRequest(method, [SIGNER, PERMIT], SIGNER).ok, method).toBe(true);
      expect(validateAuthRequest(method, [SIGNER.toLowerCase(), PERMIT], SIGNER).ok, method).toBe(true);
      expect(validateAuthRequest(method, [SIGNER, PERMIT], SIGNER.toLowerCase()).ok, method).toBe(true);
      expect(validateAuthRequest(method, [SIGNER, JSON.parse(PERMIT)], SIGNER).ok, method).toBe(true);
      expect(validateAuthRequest(method, [OTHER, PERMIT], null).ok, method).toBe(true);
    }
  });

  it("rejects malformed typed-data params with -32602 whatever the method casing", () => {
    const cases: [string, unknown[] | undefined][] = [
      ["two typed-data params with the harmless one first", [STATEMENT, PERMIT]],
      ["the legacy [typed data, signer] order", [PERMIT, SIGNER]],
      ["an address that is not the signer", [OTHER, PERMIT]],
      ["a single param", [PERMIT]],
      ["a third param", [SIGNER, PERMIT, STATEMENT]],
      ["no params", undefined],
      ["typed data that is not JSON", [SIGNER, "{not json"]],
      ["a v1 field list without primaryType", [SIGNER, JSON.stringify([{ type: "string", name: "Message", value: "hi" }])]],
    ];
    for (const method of [...TYPED, "ETH_SIGNTYPEDDATA_V4"]) {
      for (const [label, params] of cases) {
        expect(rejection(method, params, SIGNER), `${method}: ${label}`).toMatchObject(MALFORMED_SIGNATURE);
      }
      expect(rejection(method, [PERMIT, SIGNER], null).kind, method).toBe("malformed_signature");
      expect(rejection(method, ["not-an-address", PERMIT], null).kind, method).toBe("malformed_signature");
    }
  });

  it("accepts a personal_sign [message, signer] and only checks the signer shape before an account is known", () => {
    expect(validateAuthRequest("personal_sign", ["hello", SIGNER], SIGNER).ok).toBe(true);
    expect(validateAuthRequest("personal_sign", ["0x68656c6c6f", SIGNER.toLowerCase()], SIGNER).ok).toBe(true);
    expect(validateAuthRequest("personal_sign", ["hello", OTHER], null).ok).toBe(true);
    expect(rejection("personal_sign", [OTHER, OTHER], null).kind).toBe("malformed_signature");
    expect(rejection("personal_sign", ["hello", "not-an-address"], null).kind).toBe("malformed_signature");
  });

  it("rejects malformed personal_sign params with -32602", () => {
    const cases: [string, unknown[]][] = [
      ["[signer, message]", [SIGNER, "hello"]],
      ["a signer that is not the connected account", ["hello", OTHER]],
      ["the signer in both positions", [SIGNER, SIGNER]],
      ["a message that is not a string", [{ text: "hello" }, SIGNER]],
      ["a single param", ["hello"]],
      ["a padded array", ["hello", SIGNER, "extra"]],
    ];
    for (const [label, params] of cases) {
      expect(rejection("personal_sign", params, SIGNER), label).toMatchObject(MALFORMED_SIGNATURE);
    }
  });
});

describe("validateAuthRequest signature payload bounds", () => {
  const method = "eth_signTypedData_v4";

  function typedDataOf(message: Record<string, unknown>): string {
    return JSON.stringify({
      domain: { name: "Token" },
      primaryType: "Permit",
      types: { Permit: [{ name: "memo", type: "string" }] },
      message,
    });
  }

  it("accepts a payload at the cap and refuses one past it, typed data or personal_sign", () => {
    const envelope = typedDataOf({ memo: "" }).length;
    const atCap = typedDataOf({ memo: "a".repeat(MAX_SIGNATURE_PAYLOAD_CHARS - envelope) });
    expect(atCap.length).toBe(MAX_SIGNATURE_PAYLOAD_CHARS);
    expect(validateAuthRequest(method, [SIGNER, atCap], SIGNER).ok).toBe(true);

    const overCap = typedDataOf({ memo: "a".repeat(MAX_SIGNATURE_PAYLOAD_CHARS - envelope + 1) });
    const result = rejection(method, [SIGNER, overCap], SIGNER);
    expect(result).toMatchObject(MALFORMED_SIGNATURE);
    expect(result.message).toContain("the typed data is too large to review");

    const oversized = { ...JSON.parse(typedDataOf({})), message: { memo: "a".repeat(MAX_SIGNATURE_PAYLOAD_CHARS) } };
    expect(rejection(method, [SIGNER, oversized], SIGNER).message).toContain("the typed data is too large to review");

    const message = "a".repeat(MAX_SIGNATURE_PAYLOAD_CHARS);
    expect(validateAuthRequest("personal_sign", [message, SIGNER], SIGNER).ok).toBe(true);
    const past = rejection("personal_sign", [`${message}a`, SIGNER], SIGNER);
    expect(past).toMatchObject(MALFORMED_SIGNATURE);
    expect(past.message).toContain("the message is too large to review");
  });

  it("refuses typed data nested past the depth a review can render", () => {
    const nest = (depth: number): unknown => (depth === 0 ? "leaf" : { next: nest(depth - 1) });
    expect(validateAuthRequest(method, [SIGNER, typedDataOf({ memo: nest(40) })], SIGNER).ok).toBe(true);

    const result = rejection(method, [SIGNER, typedDataOf({ memo: nest(80) })], SIGNER);
    expect(result).toMatchObject(MALFORMED_SIGNATURE);
    expect(result.message).toContain("the typed data is too deeply nested to review");
  });

  it("refuses a number literal the parse would rewrite and reads exact ones, strings included", () => {
    const rewritten: [string, string][] = [
      ["an unsafe integer", '{"tokenId":9007199254740993}'],
      ["a fraction", '{"amount":0.5}'],
      ["an exponent", '{"amount":1e3}'],
      ["a negative value past the safe range", '{"amount":-9007199254740993}'],
    ];
    for (const [label, message] of rewritten) {
      const result = rejection(method, [SIGNER, permitWith(message)], SIGNER);
      expect(result, label).toMatchObject(MALFORMED_SIGNATURE);
      expect(result.message, label).toContain("a number that cannot be represented exactly");
    }
    const uint256 = "210624583337114373395836055367340864637790190801098222508621955073";
    const exact: [string, string][] = [
      ["zero and small integers", '{"nonce":0,"chainId":137}'],
      ["a whole ether amount a double holds exactly", '{"value":1000000000000000000}'],
      ["the largest safe integer", '{"id":9007199254740991}'],
      ["a number inside a string, escapes included", `{"id":"${uint256}","memo":"a \\" 9007199254740993"}`],
    ];
    for (const [label, message] of exact) {
      expect(validateAuthRequest(method, [SIGNER, permitWith(message)], SIGNER).ok, label).toBe(true);
    }
  });
});

describe("validateAuthRequest transaction params", () => {
  const method = "eth_sendTransaction";

  it("accepts a transaction the wallet can fill in, calldata at the limit included", () => {
    const cases: [string, unknown[]][] = [
      ["an address, hex calldata and a hex value", [{ to: TO, data: "0xa9059cbb", value: "0x0" }]],
      ["no data and no value", [{ to: TO }]],
      ["a decimal value", [{ to: TO, value: "1000" }]],
      ["fields the wallet is left to fill in", [{ to: TO, data: "0x", gas: "0x5208", nonce: "0x1", chainId: "0x89" }]],
      ["data exactly at the limit", [{ to: TO, data: `0x${"ab".repeat(MAX_CALLDATA_BYTES)}` }]],
      ["empty calldata, which no signature rule applies to", [{ to: TO, data: "0x" }]],
    ];
    for (const [label, params] of cases) {
      expect(validateAuthRequest(method, params, SIGNER).ok, label).toBe(true);
    }
  });

  it("rejects a malformed transaction with -32602, naming a calldata alias", () => {
    const cases: [string, unknown[] | undefined][] = [
      ["no params", undefined],
      ["two params", [{ to: TO }, { to: TO }]],
      ["a string param", ["0xabcd"]],
      ["an array param", [[TO]]],
      ["calldata in extraCallData", [{ to: TO, data: "0x", extraCallData: "0xa9059cbb" }]],
      ["calldata in input", [{ to: TO, input: "0xa9059cbb" }]],
      ["a missing to", [{ data: "0x" }]],
      ["a to that is not an address", [{ to: "attacker.eth" }]],
      ["data that is not a string", [{ to: TO, data: { hidden: true } }]],
      ["odd-length hex data", [{ to: TO, data: "0xabc" }]],
      ["data that is not hex", [{ to: TO, data: "0xzz" }]],
      ["oversized data", [{ to: TO, data: `0x${"ab".repeat(MAX_CALLDATA_BYTES + 1)}` }]],
      ["a numeric value", [{ to: TO, value: 1 }]],
      ["a value that is not a quantity", [{ to: TO, value: "1 MANA" }]],
    ];
    for (const [label, params] of cases) {
      expect(rejection(method, params, SIGNER), label).toMatchObject(MALFORMED_TRANSACTION);
    }
    expect(rejection("ETH_SENDTRANSACTION", [{ to: "not-an-address" }], SIGNER).kind).toBe("malformed_transaction");
    expect(rejection(method, [{ to: TO, input: "0x" }], SIGNER).message).toContain('"input"');
  });
});

describe("buildTransactionParams", () => {
  const DISPATCHED = { to: "0xdef", data: "0x", value: "0x0" };

  it("dispatches only to, data and value, defaulted and lowercased, and drops every wallet-owned field", () => {
    expect(buildTransactionParams([{ from: "0xabc", to: "0xdef", value: "0x1", data: "0xdead" }])).toEqual([
      { to: "0xdef", data: "0xdead", value: "0x1" },
    ]);
    for (const field of [
      "from",
      "gas",
      "gasLimit",
      "gasPrice",
      "maxFeePerGas",
      "maxPriorityFeePerGas",
      "maxFeePerBlobGas",
      "nonce",
      "type",
      "chainId",
      "accessList",
      "blobVersionedHashes",
    ]) {
      expect(buildTransactionParams([{ ...DISPATCHED, [field]: "0xdeadbeef" }]), field).toEqual([DISPATCHED]);
    }
    expect(
      buildTransactionParams([
        { ...DISPATCHED, gas: "0x5208", maxFeePerGas: "42857142857142", maxPriorityFeePerGas: "42857142857142" },
      ]),
    ).toEqual([DISPATCHED]);
    expect(buildTransactionParams([{ to: "0xdef" }])).toEqual([DISPATCHED]);
    expect(
      buildTransactionParams([{ to: "0XFEF5C99885C3036E591B6E6DB52482891834A5F4", data: "0xA9059CBB" }]),
    ).toEqual([{ to: TO, data: "0xa9059cbb", value: "0x0" }]);
  });

  it("dispatches the value as the canonical hex quantity the preview showed", () => {
    expect(buildTransactionParams([{ to: "0xdef", data: "0x", value: "10000000" }])).toEqual([
      { to: "0xdef", data: "0x", value: "0x989680" },
    ]);
    expect(buildTransactionParams([{ to: "0xdef", value: "0x0003e8" }])).toEqual([
      { to: "0xdef", data: "0x", value: "0x3e8" },
    ]);
  });

  it("throws a named error for every malformed payload instead of forwarding it", () => {
    for (const field of ["input", "extraCallData"]) {
      expect(() => buildTransactionParams([{ ...DISPATCHED, [field]: "0xa9059cbb" }]), field).toThrow(field);
    }
    for (const [label, value] of [
      ["a number", 1],
      ["a unit string", "1 MANA"],
      ["an empty string", ""],
    ] as const) {
      expect(() => buildTransactionParams([{ to: "0xdef", value }]), label).toThrow(/hex or decimal quantity/);
    }
    expect(() => buildTransactionParams([JSON.stringify({ to: "0xdef" })])).toThrow(/received string/);
    for (const [label, value] of [
      ["a missing params list", undefined],
      ["an empty params list", []],
      ["a null transaction object", [null]],
      ["an array in place of the object", [["0xdef"]]],
    ] as const) {
      expect(() => buildTransactionParams(value as unknown[] | undefined), label).toThrow(/must be an object/);
    }
    expect(() => buildTransactionParams([{ data: "0x" }])).toThrow(/missing a "to" address/);
    expect(() => buildTransactionParams([{ to: 42 }])).toThrow(/missing a "to" address/);
    for (const [label, data] of [
      ["a number", 1],
      ["a byte array", [0, 1]],
      ["an object", { data: "0x" }],
      ["a boolean", true],
    ] as const) {
      expect(() => buildTransactionParams([{ to: TO, data }]), label).toThrow(/"data" must be hex-encoded bytes/);
    }
  });
});

describe("previewTransaction", () => {
  it("shows the lowercased dispatched fields, names every recovered field left behind and fails on a calldata alias", () => {
    expect(
      previewTransaction([{ to: "0xdef", data: "0x", value: "0x0", gas: "0x5208", nonce: "0x1", from: "0xabc" }]),
    ).toEqual({ shown: { to: "0xdef", data: "0x", value: "0x0" }, dropped: ["gas", "nonce", "from"] });
    expect(previewTransaction([{ to: "0xdef" }])).toEqual({
      shown: { to: "0xdef", data: "0x", value: "0x0" },
      dropped: [],
    });
    expect(previewTransaction([{ to: "0XFEF5C99885C3036E591B6E6DB52482891834A5F4" }]).shown.to).toBe(TO);
    expect(() => previewTransaction([{ to: "0xdef", input: "0xa9059cbb" }])).toThrow("input");
  });
});

describe("rejectionOutcome", () => {
  it("reports the rejection's own code and falls back to -32602 without one", () => {
    const unsupported = rejection("eth_sign", ["0x1234", SIGNER], SIGNER);
    expect(rejectionOutcome(unsupported)).toEqual({ code: RPC_METHOD_NOT_SUPPORTED, message: unsupported.message });
    const mismatched = buildWalletRequest({ method: "eth_signTypedData_v4", params: [OTHER, PERMIT] }, SIGNER);
    if (mismatched.ok) throw new Error("expected a signer mismatch");
    expect(rejectionOutcome(mismatched.rejection)).toEqual({ code: RPC_INVALID_PARAMS, message: mismatched.rejection.message });
    const bare = { kind: "malformed_signature", message: "bad" } as unknown as RequestRejection;
    expect(rejectionOutcome(bare)).toEqual({ code: RPC_INVALID_PARAMS, message: "bad" });
  });
});

describe("buildWalletRequest", () => {
  it("dispatches the reviewed fields as the connected account and refuses a foreign signer", () => {
    expect(
      buildWalletRequest(
        { method: "eth_sendTransaction", params: [{ to: TO, data: "0xa9059cbb", gas: "0x5208", nonce: "0x1", from: OTHER }] },
        SIGNER,
      ),
    ).toEqual({
      ok: true,
      request: { method: "eth_sendTransaction", params: [{ to: TO, data: "0xa9059cbb", value: "0x0", from: SIGNER }] },
    });
    expect(buildWalletRequest({ method: "personal_sign", params: ["hello", SIGNER] }, SIGNER.toLowerCase())).toEqual({
      ok: true,
      request: { method: "personal_sign", params: ["hello", SIGNER] },
    });
    const result = buildWalletRequest({ method: "eth_signTypedData_v4", params: [OTHER, PERMIT] }, SIGNER);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.rejection).toMatchObject(MALFORMED_SIGNATURE);
  });
});

describe("decodeSignatureMessage", () => {
  it("decodes even-length hex as UTF-8 and leaves everything else alone", () => {
    expect(decodeSignatureMessage(`0x${toHex("hello")}`)).toBe("hello");
    expect(decodeSignatureMessage(`0X${toHex("hello")}`)).toBe("hello");
    expect(decodeSignatureMessage("hello")).toBe("hello");
    expect(decodeSignatureMessage("0x")).toBe("0x");
    expect(decodeSignatureMessage("0x1z")).toBe("0x1z");
    expect(decodeSignatureMessage(42)).toBeNull();
  });
});

describe("isOpaqueSignatureMessage", () => {
  it("reads text a person can check as text", () => {
    const readable: [string, string][] = [
      ["a readable sentence", "Sign in to Decentraland\nNonce: 1234"],
      ["emoji and non-latin text", "Bienvenido \u{1F44B} \u{2014} \u{6B22}\u{8FCE}"],
      [
        "a long readable sign-in message",
        "decentraland.org wants you to sign in with your Ethereum account.\n\nURI: https://decentraland.org\nVersion: 1",
      ],
      ["a short nonce-like token", "nonce-1234"],
      ["text with a non-breaking space", "Sign\u00a0in to Decentraland today please"],
      ["a URL with a scheme", "https://decentraland.org/auth/requests/abc"],
      ["31 characters with no whitespace", "a".repeat(31)],
    ];
    for (const [label, message] of readable) {
      expect(isOpaqueSignatureMessage(message), label).toBe(false);
    }
  });

  it("flags what the user cannot check the meaning of, hex-decoded bytes included", () => {
    const c1 = decodeSignatureMessage(`0x${toHex("authorize")}c285${toHex("withdrawal")}`);
    expect(c1).toBe("authorize\u0085withdrawal");
    const opaque: [string, string][] = [
      ["still raw hex", `0x${"ab".repeat(32)}`],
      ["an uppercase 0X prefix", `0X${"ab".repeat(32)}`],
      ["an empty hex payload", "0x"],
      ["bytes that are not text", "abc\u0000\u0007def"],
      ["the replacement character left by invalid UTF-8", "abc\ufffddef"],
      ["a C1 control character", "authorize\u0085withdrawal"],
      ["the last C1 control character", "abc\u009fdef"],
      ["a zero-width space", "abc\u200bdef"],
      ["a bidi override", "abc\u202edef"],
      ["a byte order mark", "\ufeffabc"],
      ["a private-use code point", "abc\ue000def"],
      ["an unassigned code point", "abc\u0378def"],
      ["a line separator", "abc\u2028def"],
      ["32 printable bytes with no whitespace", "a".repeat(32)],
      ["32 printable bytes that include a space", `${"a".repeat(15)} ${"b".repeat(16)}`],
      ["32 printable bytes that include a newline", `${"a".repeat(31)}\n`],
      ["32 bytes of multibyte characters", "\u00e9".repeat(16)],
      ["a 32-byte sentence with spaces", "Sign in to Decentraland today!!!"],
      ["a 64-character unprefixed hex digest", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"],
      ["a base64 digest", "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="],
      ["a base64url digest", "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU"],
      ["a UUID", "0f8fad5b-d9cb-469f-a165-70867728950e"],
      ["a JWT-like token", "eyJhbGciOiJIUzI1NiJ9.eyJhY3Rpb24iOiJ3aXRoZHJhdyJ9.dGhpcy1pcy1ub3QtYS1yZWFsLXNpZ25hdHVyZQ"],
      ["a token surrounded by whitespace", `  ${"A".repeat(40)}\n`],
      ["hex bytes that decode to a C1 control character", c1 ?? ""],
      ["32 hex bytes that all decode to printable characters", decodeSignatureMessage(`0x${"61".repeat(32)}`) ?? ""],
    ];
    for (const [label, message] of opaque) {
      expect(isOpaqueSignatureMessage(message), label).toBe(true);
    }
  });
});

describe("unverifiableReason", () => {
  it("names why each request cannot be checked and never exempts one from the acknowledgment", () => {
    const readable = "Sign in to Decentraland\nNonce: 1234";
    const personal: [string, unknown[], string][] = [
      ["readable text", [readable, SIGNER], "unverified_message"],
      ["hex-encoded readable text", [`0x${toHex(readable)}`, SIGNER], "unverified_message"],
      ["a perfectly readable sentence", ["hello there, a perfectly readable sentence", SIGNER], "unverified_message"],
      ["opaque hex", [`0x${"ab".repeat(32)}`, SIGNER], "opaque_message"],
      ["32 printable bytes", ["a".repeat(32), SIGNER], "opaque_message"],
      ["a missing message", [undefined, SIGNER], "opaque_message"],
    ];
    for (const [label, params, reason] of personal) {
      expect(unverifiableReason("personal_sign", params), label).toBe(reason);
    }
    expect(unverifiableReason("eth_signTypedData_v4", [SIGNER, PERMIT])).toBe("unrecognized_typed_data");
    expect(unverifiableReason("eth_signTypedData_v3", [SIGNER, STATEMENT])).toBe("unrecognized_typed_data");
    expect(unverifiableReason("eth_sendTransaction", [{ to: TO }])).toBe("unsimulated_transaction");
  });
});

describe("toHexQuantity", () => {
  it("canonicalises decimal and hex quantities and throws for anything else", () => {
    for (const [value, expected] of [
      ["1000", "0x3e8"],
      ["10000000", "0x989680"],
      ["0x0003e8", "0x3e8"],
      ["0", "0x0"],
      ["0x0", "0x0"],
      ["0x00", "0x0"],
    ]) {
      expect(toHexQuantity(value), value).toBe(expected);
    }
    for (const [label, value] of [
      ["a number", 1],
      ["undefined", undefined],
      ["a unit string", "1 MANA"],
      ["odd hex without a prefix", "f4240"],
      ["a negative amount", "-1"],
    ] as const) {
      expect(() => toHexQuantity(value), label).toThrow(/hex or decimal quantity/);
    }
  });
});

describe("an uppercase 0X prefix", () => {
  it("is an address for signers and targets but never calldata", () => {
    const upper = `0X${SIGNER.slice(2)}`;
    expect(validateAuthRequest("eth_signTypedData_v4", [upper, PERMIT], null).ok).toBe(true);
    expect(validateAuthRequest("personal_sign", ["hello", upper], null).ok).toBe(true);
    expect(
      validateAuthRequest("eth_sendTransaction", [{ to: `0X${TO.slice(2)}`, data: "0xa9059cbb" }], SIGNER).ok,
    ).toBe(true);
    expect(rejection("eth_sendTransaction", [{ to: TO, data: "0XA9059CBB" }], SIGNER)).toMatchObject(MALFORMED_TRANSACTION);
  });
});

const DOMAIN_TYPE = [
  { name: "name", type: "string" },
  { name: "version", type: "string" },
  { name: "verifyingContract", type: "address" },
  { name: "salt", type: "bytes32" },
] as const;
const OFFCHAIN_META_TRANSACTION_TYPE = [
  { name: "nonce", type: "uint256" },
  { name: "from", type: "address" },
  { name: "functionData", type: "bytes" },
] as const;
const MARKETPLACE = "0xa40b1d129b8906888720686f3a01921ddf37716f";
const POLYGON_SALT = `0x${"00".repeat(31)}89`;
const ACCEPT_CALLDATA = `0xdeadbeef${"00".repeat(64)}`;

type TypedDataField = { name: string; type: string };
type MutableTypedData = {
  types: Record<string, TypedDataField[]>;
  domain: Record<string, unknown>;
  primaryType: string;
  message: Record<string, unknown>;
};

function metaTransaction(): MutableTypedData {
  return {
    types: {
      EIP712Domain: DOMAIN_TYPE.map((field) => ({ ...field })),
      MetaTransaction: OFFCHAIN_META_TRANSACTION_TYPE.map((field) => ({ ...field })),
    },
    domain: {
      name: "DecentralandMarketplacePolygon",
      version: "1.0.0",
      verifyingContract: MARKETPLACE,
      salt: POLYGON_SALT,
    },
    primaryType: "MetaTransaction",
    message: { nonce: 0, from: SIGNER, functionData: ACCEPT_CALLDATA },
  };
}

function withoutDomainStruct(): MutableTypedData {
  const typedData = metaTransaction();
  delete (typedData.types as Record<string, unknown>).EIP712Domain;
  return typedData;
}

const CONFORMING_META_TRANSACTIONS: [string, () => unknown][] = [
  ["the payload decentraland-transactions builds", () => metaTransaction()],
  [
    "a domain struct listing the same fields in another order",
    () => {
      const typedData = metaTransaction();
      typedData.types.EIP712Domain = [...typedData.types.EIP712Domain].reverse();
      return typedData;
    },
  ],
  [
    "a primary type that only differs in casing",
    () => {
      const typedData = metaTransaction();
      typedData.primaryType = "metatransaction";
      typedData.domain.extra = "unsigned";
      return typedData;
    },
  ],
];

const DEVIATING_META_TRANSACTIONS: [string, () => unknown][] = [
  [
    "the struct omits a field the domain carries",
    () => {
      const typedData = metaTransaction();
      typedData.types.EIP712Domain = typedData.types.EIP712Domain.filter((field) => field.name !== "salt");
      return typedData;
    },
  ],
  [
    "the struct declares a field the domain lacks",
    () => {
      const typedData = metaTransaction();
      typedData.types.EIP712Domain.push({ name: "chainId", type: "uint256" });
      return typedData;
    },
  ],
  [
    "the struct repeats one field in place of another",
    () => {
      const typedData = metaTransaction();
      const [first, , third, fourth] = typedData.types.EIP712Domain;
      typedData.types.EIP712Domain = [first!, first!, third!, fourth!];
      return typedData;
    },
  ],
  [
    "the struct repeats a field the domain does have",
    () => {
      const typedData = metaTransaction();
      typedData.types.EIP712Domain.push({ ...typedData.types.EIP712Domain[0]! });
      return typedData;
    },
  ],
  [
    "the struct declares a field under a type the contract does not hash",
    () => {
      const typedData = metaTransaction();
      typedData.types.EIP712Domain = typedData.types.EIP712Domain.map((field) =>
        field.name === "salt" ? { name: "salt", type: "uint256" } : field,
      );
      return typedData;
    },
  ],
  [
    "a struct entry is not a named field",
    () => {
      const typedData = metaTransaction();
      typedData.types.EIP712Domain = [...typedData.types.EIP712Domain.slice(1), "salt" as unknown as TypedDataField];
      return typedData;
    },
  ],
  ["it carries no types, domain or message at all", () => ({ primaryType: "MetaTransaction" })],
  ["it carries a domain that is not a record", () => ({ ...metaTransaction(), domain: MARKETPLACE })],
  [
    "it carries types as an array rather than a record",
    () => ({ ...metaTransaction(), types: DOMAIN_TYPE.map((field) => ({ ...field })) }),
  ],
  ["it declares no domain struct at all", withoutDomainStruct],
  [
    "it carries a domain field EIP-712 does not define",
    () => {
      const typedData = metaTransaction();
      typedData.domain.extra = "unsigned";
      return typedData;
    },
  ],
];

describe("validateAuthRequest serves every MetaTransaction behind the acknowledgment", () => {
  it("accepts conforming, deviating and malformed MetaTransactions alike, as objects or JSON, behind unrecognized_typed_data", () => {
    const method = "eth_signTypedData_v4";
    const builds: [string, () => unknown][] = [
      ...CONFORMING_META_TRANSACTIONS,
      ...DEVIATING_META_TRANSACTIONS,
    ];
    for (const [label, build] of builds) {
      for (const typedData of [build(), JSON.stringify(build())]) {
        expect(validateAuthRequest(method, [SIGNER, typedData], SIGNER).ok, label).toBe(true);
        expect(unverifiableReason(method, [SIGNER, typedData]), label).toBe("unrecognized_typed_data");
      }
    }
    expect(validateAuthRequest(method, [SIGNER, PERMIT], SIGNER).ok).toBe(true);
    expect(unverifiableReason(method, [SIGNER, PERMIT])).toBe("unrecognized_typed_data");
  });
});

describe("metaTransactionDomainProblem", () => {
  it("passes the payload decentraland-transactions builds and judges nothing whose primary type is not exactly MetaTransaction", () => {
    for (const [label, build] of CONFORMING_META_TRANSACTIONS) {
      expect(metaTransactionDomainProblem(build()), label).toBeNull();
    }
    expect(metaTransactionDomainProblem(PERMIT)).toBeNull();
  });

  it("reports a problem for whatever deviates from the domain the contract hashes", () => {
    for (const [label, build] of DEVIATING_META_TRANSACTIONS) {
      expect(metaTransactionDomainProblem(build()), label).not.toBeNull();
    }
  });
});

describe("validateAuthRequest against the typed data Decentraland's dApps sign", () => {
  it("accepts each fixture as a string or an object behind the acknowledgment", () => {
    const method = "eth_signTypedData_v4";
    for (const [label, typedData] of Object.entries(SIGNED_BY_DAPPS)) {
      const params = [DAPP_USER, JSON.stringify(typedData)];
      expect(validateAuthRequest(method, params, DAPP_USER).ok, label).toBe(true);
      expect(validateAuthRequest(method, [DAPP_USER, typedData], DAPP_USER).ok, label).toBe(true);
      expect(unverifiableReason(method, params), label).toBe("unrecognized_typed_data");
    }
  });
});

describe("denialSender", () => {
  it("answers as the request's sender, else the connected account, else nobody", () => {
    const CONNECTED = "0x000000000000000000000000000000000000c0de";
    const cases: [string, string | undefined, string | null, string][] = [
      ["the request's sender when it matches the connected account", SIGNER, SIGNER, SIGNER],
      ["the request's sender even when another account is connected", SIGNER, CONNECTED, SIGNER],
      ["the request's sender when no account is connected", SIGNER, null, SIGNER],
      ["the connected account when the request names no sender", undefined, CONNECTED, CONNECTED],
      ["the connected account when the request's sender is blank", "", CONNECTED, CONNECTED],
      ["nothing when the request's sender is blank and no account is connected", "   ", null, ""],
      ["nothing when neither is known", undefined, null, ""],
    ];
    for (const [label, sender, connected, expected] of cases) {
      expect(denialSender({ sender } as { sender?: string | null }, connected), label).toBe(expected);
    }
    expect(denialSender(null, null)).toBe("");
    expect(denialSender(undefined, CONNECTED)).toBe(CONNECTED);
  });
});

describe("isSameAccount", () => {
  it("compares accounts by their bytes and refuses anything unknown or different", () => {
    expect(isSameAccount(SIGNER, SIGNER.toLowerCase())).toBe(true);
    expect(isSameAccount(` ${SIGNER} `, SIGNER.toUpperCase())).toBe(true);
    const refused: [string, string | null, string | null][] = [
      ["a different account", SIGNER, OTHER],
      ["an unknown live account", SIGNER, null],
      ["an unknown expected account", null, SIGNER],
      ["two unknown accounts", null, null],
      ["an empty account", "", ""],
    ];
    for (const [label, a, b] of refused) {
      expect(isSameAccount(a, b), label).toBe(false);
    }
  });
});

describe("approveBlocked", () => {
  it("holds Approve while signing or while any applicable gate is unticked", () => {
    const gates = (overrides: Partial<ApprovalGates> = {}): ApprovalGates => ({
      isSigning: false,
      mustValidate: false,
      acknowledged: false,
      unverifiable: null,
      effectsAcknowledged: false,
      ...overrides,
    });
    const cases: [string, ApprovalGates, boolean][] = [
      ["nothing to acknowledge", gates(), false],
      ["a signature already in flight", gates({ isSigning: true }), true],
      ["an unticked code match", gates({ mustValidate: true }), true],
      ["a ticked code match", gates({ mustValidate: true, acknowledged: true }), false],
      ["unacknowledged effects", gates({ unverifiable: "unsimulated_transaction" }), true],
      ["acknowledged effects", gates({ unverifiable: "unsimulated_transaction", effectsAcknowledged: true }), false],
      ["one of two gates ticked", gates({ mustValidate: true, acknowledged: true, unverifiable: "opaque_message" }), true],
      ["the other of two gates ticked", gates({ mustValidate: true, unverifiable: "opaque_message", effectsAcknowledged: true }), true],
      [
        "both gates ticked",
        gates({ mustValidate: true, acknowledged: true, unverifiable: "opaque_message", effectsAcknowledged: true }),
        false,
      ],
      [
        "both gates ticked while signing",
        gates({ isSigning: true, mustValidate: true, acknowledged: true, unverifiable: "opaque_message", effectsAcknowledged: true }),
        true,
      ],
    ];
    for (const [label, input, expected] of cases) {
      expect(approveBlocked(input), label).toBe(expected);
    }
  });
});
