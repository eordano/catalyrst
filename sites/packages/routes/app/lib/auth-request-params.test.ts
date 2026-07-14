import { describe, expect, it } from "vitest";

import {
  MAX_CALLDATA_BYTES,
  RPC_INVALID_PARAMS,
  RPC_METHOD_NOT_SUPPORTED,
  buildTransactionParams,
  buildWalletRequest,
  canonicalMethod,
  decodeSignatureMessage,
  isEphemeralMessage,
  isOpaqueSignatureMessage,
  isRetiredSignInMethod,
  previewTransaction,
  rejectionOutcome,
  unverifiableReason,
  validateAuthRequest,
  type RequestRejection,
} from "./auth-request-params";

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

function rejection(method: string, params: unknown[] | undefined, signer: string | null): RequestRejection {
  const result = validateAuthRequest(method, params, signer);
  if (result.ok) throw new Error(`expected ${method} to be rejected`);
  return result.rejection;
}

function toHex(text: string): string {
  return Array.from(new TextEncoder().encode(text))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

describe("canonicalMethod", () => {
  it.each(["personal_sign", "eth_signTypedData_v3", "eth_signTypedData_v4", "eth_sendTransaction"])(
    "returns %s unchanged",
    (method) => {
      expect(canonicalMethod(method)).toBe(method);
    },
  );

  it("pins an oddly cased method to its canonical spelling", () => {
    expect(canonicalMethod("ETH_SENDTRANSACTION")).toBe("eth_sendTransaction");
    expect(canonicalMethod(" Personal_Sign ")).toBe("personal_sign");
  });

  it.each(["eth_signTypedData", "eth_sign", "dcl_personal_sign", "wallet_addEthereumChain", ""])(
    "refuses %s",
    (method) => {
      expect(canonicalMethod(method)).toBeNull();
    },
  );

  it("recognizes the retired sign-in method for the outdated-client copy", () => {
    expect(isRetiredSignInMethod("dcl_personal_sign")).toBe(true);
    expect(isRetiredSignInMethod("DCL_PERSONAL_SIGN")).toBe(true);
    expect(isRetiredSignInMethod("personal_sign")).toBe(false);
  });
});

describe("validateAuthRequest method and impersonation guards", () => {
  it("reports an unsupported method with -32601", () => {
    expect(rejection("eth_sign", [SIGNER, "0x00"], SIGNER)).toMatchObject({
      code: RPC_METHOD_NOT_SUPPORTED,
      kind: "unsupported_method",
    });
  });

  it("reports the retired sign-in separately so the page can say the app is out of date", () => {
    expect(rejection("dcl_personal_sign", ["hello", SIGNER], SIGNER)).toMatchObject({
      code: RPC_METHOD_NOT_SUPPORTED,
      kind: "retired_sign_in",
    });
  });

  it("drops the legacy eth_signTypedData v1 from the accepted set", () => {
    expect(rejection("eth_signTypedData", [SIGNER, PERMIT], SIGNER).code).toBe(RPC_METHOD_NOT_SUPPORTED);
  });

  it("blocks an identity payload in plaintext or hex with -32602", () => {
    const payload = `Decentraland Login\nEphemeral address: 0x${"ab".repeat(20)}\nExpiration: 2030-01-01T00:00:00.000Z`;
    expect(rejection("personal_sign", [payload, SIGNER], SIGNER)).toMatchObject({
      code: RPC_INVALID_PARAMS,
      kind: "impersonated_sign_in",
    });
    expect(rejection("personal_sign", [`0x${toHex(payload)}`, SIGNER], SIGNER).kind).toBe("impersonated_sign_in");
    expect(isEphemeralMessage(`Anything\nEphemeral address: 0xabc\nExpiration: soon`)).toBe(true);
    expect(isEphemeralMessage("Decentraland Login\nnot an ephemeral payload")).toBe(false);
  });
});

describe("validateAuthRequest signature params", () => {
  describe.each(["eth_signTypedData_v4", "eth_signTypedData_v3"])("with %s", (method) => {
    it("accepts [signer, typed data]", () => {
      expect(validateAuthRequest(method, [SIGNER, PERMIT], SIGNER).ok).toBe(true);
    });

    it("compares the signer case-insensitively", () => {
      expect(validateAuthRequest(method, [SIGNER.toLowerCase(), PERMIT], SIGNER).ok).toBe(true);
      expect(validateAuthRequest(method, [SIGNER, PERMIT], SIGNER.toLowerCase()).ok).toBe(true);
    });

    it("accepts typed data passed as an object", () => {
      expect(validateAuthRequest(method, [SIGNER, JSON.parse(PERMIT)], SIGNER).ok).toBe(true);
    });

    it.each([
      ["two typed-data params with the harmless one first", [STATEMENT, PERMIT]],
      ["the legacy [typed data, signer] order", [PERMIT, SIGNER]],
      ["an address that is not the signer", [OTHER, PERMIT]],
      ["a single param", [PERMIT]],
      ["a third param", [SIGNER, PERMIT, STATEMENT]],
      ["no params", undefined],
      ["typed data that is not JSON", [SIGNER, "{not json"]],
      ["a v1 field list without primaryType", [SIGNER, JSON.stringify([{ type: "string", name: "Message", value: "hi" }])]],
    ])("rejects %s with -32602", (_label, params) => {
      expect(rejection(method, params as unknown[] | undefined, SIGNER)).toMatchObject({
        code: RPC_INVALID_PARAMS,
        kind: "malformed_signature",
      });
    });

    it("only checks the signer shape before any account is known", () => {
      expect(validateAuthRequest(method, [OTHER, PERMIT], null).ok).toBe(true);
      expect(rejection(method, [PERMIT, SIGNER], null).kind).toBe("malformed_signature");
      expect(rejection(method, ["not-an-address", PERMIT], null).kind).toBe("malformed_signature");
    });
  });

  it("still validates the typed-data shape when the method casing differs", () => {
    expect(rejection("ETH_SIGNTYPEDDATA_V4", [STATEMENT, PERMIT], SIGNER).kind).toBe("malformed_signature");
  });

  describe("with personal_sign", () => {
    it("accepts [message, signer]", () => {
      expect(validateAuthRequest("personal_sign", ["hello", SIGNER], SIGNER).ok).toBe(true);
      expect(validateAuthRequest("personal_sign", ["0x68656c6c6f", SIGNER.toLowerCase()], SIGNER).ok).toBe(true);
    });

    it.each([
      ["[signer, message]", [SIGNER, "hello"]],
      ["a signer that is not the connected account", ["hello", OTHER]],
      ["the signer in both positions", [SIGNER, SIGNER]],
      ["a message that is not a string", [{ text: "hello" }, SIGNER]],
      ["a single param", ["hello"]],
      ["a padded array", ["hello", SIGNER, "extra"]],
    ])("rejects %s with -32602", (_label, params) => {
      expect(rejection("personal_sign", params as unknown[], SIGNER)).toMatchObject({
        code: RPC_INVALID_PARAMS,
        kind: "malformed_signature",
      });
    });

    it("only checks the signer shape before any account is known", () => {
      expect(validateAuthRequest("personal_sign", ["hello", OTHER], null).ok).toBe(true);
      expect(rejection("personal_sign", [OTHER, OTHER], null).kind).toBe("malformed_signature");
      expect(rejection("personal_sign", ["hello", "not-an-address"], null).kind).toBe("malformed_signature");
    });
  });

  it("does not hold eth_sendTransaction to the signature rules", () => {
    expect(validateAuthRequest("eth_sendTransaction", [{ to: TO, data: "0x" }], SIGNER).ok).toBe(true);
  });
});

describe("validateAuthRequest transaction params", () => {
  const method = "eth_sendTransaction";

  it.each([
    ["an address, hex calldata and a hex value", [{ to: TO, data: "0xa9059cbb", value: "0x0" }]],
    ["no data and no value", [{ to: TO }]],
    ["a decimal value", [{ to: TO, value: "1000" }]],
    ["fields the wallet is left to fill in", [{ to: TO, data: "0x", gas: "0x5208", nonce: "0x1", chainId: "0x89" }]],
    ["data exactly at the limit", [{ to: TO, data: `0x${"ab".repeat(MAX_CALLDATA_BYTES)}` }]],
  ])("accepts %s", (_label, params) => {
    expect(validateAuthRequest(method, params, SIGNER).ok).toBe(true);
  });

  it("still validates the transaction when the method casing differs", () => {
    expect(rejection("ETH_SENDTRANSACTION", [{ to: "not-an-address" }], SIGNER).kind).toBe("malformed_transaction");
  });

  it.each([
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
  ])("rejects %s with -32602", (_label, params) => {
    expect(rejection(method, params as unknown[] | undefined, SIGNER)).toMatchObject({
      code: RPC_INVALID_PARAMS,
      kind: "malformed_transaction",
    });
  });

  it("names the alias in the rejection message", () => {
    expect(rejection(method, [{ to: TO, input: "0x" }], SIGNER).message).toContain('"input"');
  });
});

describe("buildTransactionParams", () => {
  it("keeps to, data and value and drops from", () => {
    expect(buildTransactionParams([{ from: "0xabc", to: "0xdef", value: "0x1", data: "0xdead" }])).toEqual([
      { to: "0xdef", data: "0xdead", value: "0x1" },
    ]);
  });

  it.each([
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
  ])("drops %s so it never reaches the wallet", (field) => {
    expect(buildTransactionParams([{ to: "0xdef", data: "0x", value: "0x0", [field]: "0xdeadbeef" }])).toEqual([
      { to: "0xdef", data: "0x", value: "0x0" },
    ]);
  });

  it("drops inflated EIP-1559 fees so the wallet sets the network fee", () => {
    expect(
      buildTransactionParams([
        { to: "0xdef", data: "0x", value: "0x0", gas: "0x5208", maxFeePerGas: "42857142857142", maxPriorityFeePerGas: "42857142857142" },
      ]),
    ).toEqual([{ to: "0xdef", data: "0x", value: "0x0" }]);
  });

  it.each(["input", "extraCallData"])("rejects %s instead of silently dropping it", (field) => {
    expect(() => buildTransactionParams([{ to: "0xdef", data: "0x", value: "0x0", [field]: "0xa9059cbb" }])).toThrow(field);
  });

  it("defaults data and value", () => {
    expect(buildTransactionParams([{ to: "0xdef" }])).toEqual([{ to: "0xdef", data: "0x", value: "0x0" }]);
  });

  it("names the type when the caller serialised the payload", () => {
    expect(() => buildTransactionParams([JSON.stringify({ to: "0xdef" })])).toThrow(/received string/);
  });

  it.each([
    ["a missing params list", undefined],
    ["an empty params list", []],
    ["a null transaction object", [null]],
    ["an array in place of the object", [["0xdef"]]],
  ])("throws for %s", (_label, value) => {
    expect(() => buildTransactionParams(value as unknown[] | undefined)).toThrow(/must be an object/);
  });

  it("says the address is missing rather than blaming the shape", () => {
    expect(() => buildTransactionParams([{ data: "0x" }])).toThrow(/missing a "to" address/);
    expect(() => buildTransactionParams([{ to: 42 }])).toThrow(/missing a "to" address/);
  });
});

describe("previewTransaction", () => {
  it("shows the dispatched fields and names every recovered field left behind", () => {
    expect(
      previewTransaction([{ to: "0xdef", data: "0x", value: "0x0", gas: "0x5208", nonce: "0x1", from: "0xabc" }]),
    ).toEqual({ shown: { to: "0xdef", data: "0x", value: "0x0" }, dropped: ["gas", "nonce", "from"] });
  });

  it("names nothing when the recovered transaction only carries the reviewed fields", () => {
    expect(previewTransaction([{ to: "0xdef" }])).toEqual({
      shown: { to: "0xdef", data: "0x", value: "0x0" },
      dropped: [],
    });
  });

  it("fails the same way buildTransactionParams does on a calldata alias", () => {
    expect(() => previewTransaction([{ to: "0xdef", input: "0xa9059cbb" }])).toThrow("input");
  });
});

describe("rejectionOutcome", () => {
  it("reports the rejection's own code", () => {
    const unsupported = rejection("eth_sign", ["0x1234", SIGNER], SIGNER);
    expect(rejectionOutcome(unsupported)).toEqual({ code: RPC_METHOD_NOT_SUPPORTED, message: unsupported.message });
    const mismatched = buildWalletRequest({ method: "eth_signTypedData_v4", params: [OTHER, PERMIT] }, SIGNER);
    if (mismatched.ok) throw new Error("expected a signer mismatch");
    expect(rejectionOutcome(mismatched.rejection)).toEqual({ code: RPC_INVALID_PARAMS, message: mismatched.rejection.message });
  });

  it("falls back to -32602 when the rejection carries no code", () => {
    const bare = { kind: "malformed_signature", message: "bad" } as unknown as RequestRejection;
    expect(rejectionOutcome(bare)).toEqual({ code: RPC_INVALID_PARAMS, message: "bad" });
  });
});

describe("buildWalletRequest", () => {
  it("dispatches only the reviewed transaction fields with the connected account as from", () => {
    const result = buildWalletRequest(
      { method: "eth_sendTransaction", params: [{ to: TO, data: "0xa9059cbb", gas: "0x5208", nonce: "0x1", from: OTHER }] },
      SIGNER,
    );
    expect(result).toEqual({
      ok: true,
      request: { method: "eth_sendTransaction", params: [{ to: TO, data: "0xa9059cbb", value: "0x0", from: SIGNER }] },
    });
  });

  it("forwards signature params verbatim once the connected account is the signer", () => {
    expect(buildWalletRequest({ method: "personal_sign", params: ["hello", SIGNER] }, SIGNER.toLowerCase())).toEqual({
      ok: true,
      request: { method: "personal_sign", params: ["hello", SIGNER] },
    });
  });

  it("rejects a signature whose signer is not the connected account with -32602", () => {
    const result = buildWalletRequest({ method: "eth_signTypedData_v4", params: [OTHER, PERMIT] }, SIGNER);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.rejection).toMatchObject({ code: RPC_INVALID_PARAMS, kind: "malformed_signature" });
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
  it.each([
    ["a readable sentence", "Sign in to Decentraland\nNonce: 1234"],
    ["emoji and non-latin text", "Bienvenido \u{1F44B} \u{2014} \u{6B22}\u{8FCE}"],
    [
      "a long readable sign-in message",
      "decentraland.org wants you to sign in with your Ethereum account.\n\nURI: https://decentraland.org\nVersion: 1",
    ],
    ["a short nonce-like token", "nonce-1234"],
    ["text with a non-breaking space", "Sign\u00A0in to Decentraland today please"],
    ["a URL with a scheme", "https://decentraland.org/auth/requests/abc"],
    ["31 characters with no whitespace", "a".repeat(31)],
  ])("reads %s as text", (_label, message) => {
    expect(isOpaqueSignatureMessage(message)).toBe(false);
  });

  it.each([
    ["still raw hex", `0x${"ab".repeat(32)}`],
    ["an empty hex payload", "0x"],
    ["bytes that are not text", "abc\u0000\u0007def"],
    ["the replacement character left by invalid UTF-8", "abc\uFFFDdef"],
    ["a C1 control character", "authorize\u0085withdrawal"],
    ["the last C1 control character", "abc\u009Fdef"],
    ["a zero-width space", "abc\u200Bdef"],
    ["a bidi override", "abc\u202Edef"],
    ["a byte order mark", "\uFEFFabc"],
    ["a private-use code point", "abc\uE000def"],
    ["an unassigned code point", "abc\u0378def"],
    ["a line separator", "abc\u2028def"],
    ["32 printable bytes with no whitespace", "a".repeat(32)],
    ["32 printable bytes that include a space", `${"a".repeat(15)} ${"b".repeat(16)}`],
    ["32 printable bytes that include a newline", `${"a".repeat(31)}\n`],
    ["32 bytes of multibyte characters", "\u00E9".repeat(16)],
    ["a 32-byte sentence with spaces", "Sign in to Decentraland today!!!"],
    ["a 64-character unprefixed hex digest", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"],
    ["a base64 digest", "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="],
    ["a base64url digest", "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU"],
    ["a UUID", "0f8fad5b-d9cb-469f-a165-70867728950e"],
    ["a JWT-like token", "eyJhbGciOiJIUzI1NiJ9.eyJhY3Rpb24iOiJ3aXRoZHJhdyJ9.dGhpcy1pcy1ub3QtYS1yZWFsLXNpZ25hdHVyZQ"],
    ["a token surrounded by whitespace", `  ${"A".repeat(40)}\n`],
  ])("flags %s because the user cannot check what it means", (_label, message) => {
    expect(isOpaqueSignatureMessage(message)).toBe(true);
  });

  it("flags hex bytes that decode to a C1 control character", () => {
    const message = decodeSignatureMessage(`0x${toHex("authorize")}c285${toHex("withdrawal")}`);
    expect(message).toBe("authorize\u0085withdrawal");
    expect(isOpaqueSignatureMessage(message ?? "")).toBe(true);
  });

  it("flags 32 hex bytes that all decode to printable characters", () => {
    expect(isOpaqueSignatureMessage(decodeSignatureMessage(`0x${"61".repeat(32)}`) ?? "")).toBe(true);
  });
});

describe("unverifiableReason", () => {
  it("lets readable personal_sign text through", () => {
    expect(unverifiableReason("personal_sign", ["Sign in to Decentraland\nNonce: 1234", SIGNER])).toBeNull();
    expect(unverifiableReason("personal_sign", [`0x${toHex("Sign in to Decentraland\nNonce: 1234")}`, SIGNER])).toBeNull();
  });

  it("flags an opaque personal_sign message, hex-encoded or not", () => {
    expect(unverifiableReason("personal_sign", [`0x${"ab".repeat(32)}`, SIGNER])).toBe("opaque_message");
    expect(unverifiableReason("personal_sign", ["a".repeat(32), SIGNER])).toBe("opaque_message");
    expect(unverifiableReason("personal_sign", [undefined, SIGNER])).toBe("opaque_message");
  });

  it("fails closed on every typed-data signature and every transaction", () => {
    expect(unverifiableReason("eth_signTypedData_v4", [SIGNER, PERMIT])).toBe("unrecognized_typed_data");
    expect(unverifiableReason("eth_signTypedData_v3", [SIGNER, STATEMENT])).toBe("unrecognized_typed_data");
    expect(unverifiableReason("eth_sendTransaction", [{ to: TO }])).toBe("unsimulated_transaction");
  });
});
