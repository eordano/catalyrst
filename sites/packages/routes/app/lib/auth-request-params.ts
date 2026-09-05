// Mirrors auth/src/shared/auth/signMethodGuard.ts, RequestPage/transactionParams.ts and the
// isOpaqueSignatureMessage rule of RequestPage/utils.ts: what a recovered request must satisfy
// before the page shows it, and the exact shape the wallet is finally handed. Every rule fails
// closed; the page has no simulation or contract registry to fall back on.

export const RPC_METHOD_NOT_SUPPORTED = -32601;
export const RPC_INVALID_PARAMS = -32602;
export const RPC_USER_REJECTED = -32003;

// dcl_personal_sign (retired sign-in), eth_sign (raw digest, no EIP-191 prefix) and the v1
// eth_signTypedData (reversed params, no client uses it) must never be added back.
const ALLOWED_METHODS = [
  "personal_sign",
  "eth_signTypedData_v3",
  "eth_signTypedData_v4",
  "eth_sendTransaction",
] as const;
export type AllowedMethod = (typeof ALLOWED_METHODS)[number];

const CANONICAL_BY_LOWERCASE = new Map<string, AllowedMethod>(
  ALLOWED_METHODS.map((method) => [method.toLowerCase(), method]),
);
const RETIRED_SIGN_IN_METHOD = "dcl_personal_sign";

export function canonicalMethod(method: string): AllowedMethod | null {
  return CANONICAL_BY_LOWERCASE.get(method.trim().toLowerCase()) ?? null;
}

export function isRetiredSignInMethod(method: string): boolean {
  return method.trim().toLowerCase() === RETIRED_SIGN_IN_METHOD;
}

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;
const HEX_BYTES_RE = /^0[xX]([0-9a-fA-F]{2})*$/;

export function decodeHexMessage(value: string): string | null {
  if (!HEX_BYTES_RE.test(value) || value.length <= 2) return null;
  const body = value.slice(2);
  const bytes = new Uint8Array(body.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = Number.parseInt(body.slice(i * 2, i * 2 + 2), 16);
  }
  return new TextDecoder().decode(bytes);
}

// The wallet signs the decoded bytes, so the text the user reads is the decoded form whenever
// the param decodes; a raw param that is not hex stays as it is.
export function decodeSignatureMessage(value: unknown): string | null {
  if (typeof value !== "string") return null;
  return decodeHexMessage(value) ?? value;
}

// Mirrors catalyrst-explorer-api auth_api/validation.rs and @dcl/crypto's parseEmphemeralPayload:
// only the 2nd and 3rd lines decide whether a signature yields a usable auth chain, so the
// header line is ignored on purpose.
const EPHEMERAL_ADDRESS_PREFIX = "Ephemeral address: ";
const EXPIRATION_PREFIX = "Expiration: ";

function isEphemeralText(value: string): boolean {
  const lines = value.replace(/\r/g, "").split("\n").slice(1);
  const addressLine = lines[0];
  const expirationLine = lines[1];
  if (addressLine === undefined || expirationLine === undefined) return false;
  if (!addressLine.startsWith(EPHEMERAL_ADDRESS_PREFIX)) return false;
  if (!expirationLine.startsWith(EXPIRATION_PREFIX)) return false;
  return expirationLine.slice(EXPIRATION_PREFIX.length).trim().length > 0;
}

export function isEphemeralMessage(value: unknown): boolean {
  if (typeof value !== "string") return false;
  if (isEphemeralText(value)) return true;
  const decoded = decodeHexMessage(value);
  return decoded !== null && isEphemeralText(decoded);
}

export type RejectionKind =
  | "retired_sign_in"
  | "unsupported_method"
  | "impersonated_sign_in"
  | "malformed_signature"
  | "malformed_transaction";

export type RequestRejection = {
  code: typeof RPC_METHOD_NOT_SUPPORTED | typeof RPC_INVALID_PARAMS;
  kind: RejectionKind;
  message: string;
};

export type ValidatedRequest = { method: AllowedMethod; params: unknown[] };

export type ValidationResult =
  | { ok: true; request: ValidatedRequest }
  | { ok: false; rejection: RequestRejection };

function hasPrimaryType(typedData: unknown): boolean {
  if (typeof typedData === "string") {
    try {
      typedData = JSON.parse(typedData);
    } catch {
      return false;
    }
  }
  return (
    typeof typedData === "object" &&
    typedData !== null &&
    typeof (typedData as { primaryType?: unknown }).primaryType === "string"
  );
}

// Wallets and the preview read signature params by position. With the signer known (the
// request's sender, or the account connected on approve) the element must be that address;
// before any account is known only its shape can be checked, and the connected account is
// held to the exact rule again before dispatch (see buildWalletRequest).
function signatureParamsProblem(
  method: AllowedMethod,
  params: unknown[] | undefined,
  signer: string | null,
): string | null {
  if (method === "eth_sendTransaction") return null;
  if (!Array.isArray(params) || params.length !== 2) return "expected exactly two params";
  const [first, second] = params;
  const expected = signer?.toLowerCase() ?? null;
  const isSigner = (param: unknown): boolean =>
    typeof param === "string" &&
    (expected !== null ? param.toLowerCase() === expected : ADDRESS_RE.test(param));

  if (method === "personal_sign") {
    if (typeof first !== "string" || typeof second !== "string" || !isSigner(second)) {
      return "expected [message, signer]";
    }
    const firstIsSigner =
      expected !== null ? isSigner(first) : first.toLowerCase() === second.toLowerCase();
    return firstIsSigner ? "expected [message, signer]" : null;
  }

  if (!isSigner(first)) return "expected [signer, typedData]";
  if (!hasPrimaryType(second)) return "typed data must declare a primaryType";
  return null;
}

// Fields other than `data` that carry calldata: viem forwards `input` and thirdweb appends
// `extraCallData`, so either would execute bytes the page never showed.
const CALLDATA_ALIASES = ["input", "extraCallData"] as const;
const CALLDATA_RE = /^0x([0-9a-fA-F]{2})*$/;
const QUANTITY_RE = /^(0x[0-9a-fA-F]{1,64}|[0-9]{1,78})$/;
export const MAX_CALLDATA_BYTES = 96 * 1024;

function calldataAlias(fields: Record<string, unknown>): string | null {
  return CALLDATA_ALIASES.find((key) => fields[key] !== undefined) ?? null;
}

function transactionParamsProblem(
  method: AllowedMethod,
  params: unknown[] | undefined,
): string | null {
  if (method !== "eth_sendTransaction") return null;
  if (!Array.isArray(params) || params.length !== 1) {
    return "expected exactly one transaction object";
  }
  const transaction: unknown = params[0];
  if (typeof transaction !== "object" || transaction === null || Array.isArray(transaction)) {
    return "the transaction must be an object";
  }
  const fields = transaction as Record<string, unknown>;
  const alias = calldataAlias(fields);
  if (alias) return `calldata must be provided in "data", not "${alias}"`;
  if (typeof fields.to !== "string" || !ADDRESS_RE.test(fields.to)) {
    return '"to" must be an address';
  }
  if (fields.data !== undefined) {
    if (typeof fields.data !== "string" || !CALLDATA_RE.test(fields.data)) {
      return '"data" must be hex-encoded bytes';
    }
    if ((fields.data.length - 2) / 2 > MAX_CALLDATA_BYTES) return '"data" is too large';
  }
  if (
    fields.value !== undefined &&
    (typeof fields.value !== "string" || !QUANTITY_RE.test(fields.value))
  ) {
    return '"value" must be a hex or decimal quantity';
  }
  return null;
}

function reject(
  code: RequestRejection["code"],
  kind: RejectionKind,
  message: string,
): ValidationResult {
  return { ok: false, rejection: { code, kind, message } };
}

export function validateAuthRequest(
  method: string,
  params: unknown[] | undefined,
  signer: string | null,
): ValidationResult {
  const canonical = canonicalMethod(method);
  if (!canonical) {
    return reject(
      RPC_METHOD_NOT_SUPPORTED,
      isRetiredSignInMethod(method) ? "retired_sign_in" : "unsupported_method",
      `The "${method}" method is not supported`,
    );
  }
  if (params?.some(isEphemeralMessage)) {
    return reject(
      RPC_INVALID_PARAMS,
      "impersonated_sign_in",
      `The "${canonical}" method cannot be used to sign a Decentraland sign-in payload`,
    );
  }
  const signature = signatureParamsProblem(canonical, params, signer);
  if (signature) {
    return reject(
      RPC_INVALID_PARAMS,
      "malformed_signature",
      `The "${canonical}" request parameters are malformed: ${signature}`,
    );
  }
  const transaction = transactionParamsProblem(canonical, params);
  if (transaction) {
    return reject(
      RPC_INVALID_PARAMS,
      "malformed_transaction",
      `The "${canonical}" transaction parameters are malformed: ${transaction}`,
    );
  }
  return { ok: true, request: { method: canonical, params: params ?? [] } };
}

const describeType = (value: unknown): string =>
  Array.isArray(value) ? "array" : value === null ? "null" : typeof value;

export type TransactionParams = { to: string; data: unknown; value: unknown };

// Only the reviewed fields reach the wallet; gas, fees, nonce, type, chainId and access lists
// are left for the wallet to fill in so what gets signed is what was shown.
export function buildTransactionParams(params: unknown[] | undefined): [TransactionParams] {
  const [txParams] = params ?? [];
  if (txParams === null || typeof txParams !== "object" || Array.isArray(txParams)) {
    throw new Error(
      `Transaction parameters must be an object, received ${describeType(txParams)}: ${JSON.stringify(txParams ?? null)}`,
    );
  }
  const source = txParams as Record<string, unknown>;
  const alias = calldataAlias(source);
  if (alias) {
    throw new Error(
      `Transaction parameter "${alias}" is not supported; calldata must be provided in "data"`,
    );
  }
  const to = source.to;
  if (typeof to !== "string" || to.length === 0) {
    throw new Error(`Transaction parameters are missing a "to" address: ${JSON.stringify(source)}`);
  }
  return [{ to, data: source.data ?? "0x", value: source.value ?? "0x0" }];
}

export type TransactionPreview = { shown: TransactionParams; dropped: string[] };

// The preview is the dispatched object itself, so the user reviews exactly what the wallet
// receives and is told which recovered fields it will never see.
export function previewTransaction(params: unknown[] | undefined): TransactionPreview {
  const [shown] = buildTransactionParams(params);
  const source = (params ?? [])[0] as Record<string, unknown>;
  return { shown, dropped: Object.keys(source).filter((key) => !(key in shown)) };
}

export type WalletRequest = { method: AllowedMethod; params: unknown[] };

export type WalletRequestResult =
  | { ok: true; request: WalletRequest }
  | { ok: false; rejection: RequestRejection };

export function buildWalletRequest(
  request: ValidatedRequest,
  sender: string,
): WalletRequestResult {
  const validated = validateAuthRequest(request.method, request.params, sender);
  if (!validated.ok) return validated;
  if (validated.request.method !== "eth_sendTransaction") return validated;
  const [transaction] = buildTransactionParams(validated.request.params);
  return {
    ok: true,
    request: { method: "eth_sendTransaction", params: [{ ...transaction, from: sender }] },
  };
}

export function rejectionOutcome(rejection: RequestRejection): { code: number; message: string } {
  return { code: rejection.code ?? RPC_INVALID_PARAMS, message: rejection.message };
}

// Anything the user cannot see or read, by Unicode category: controls, format characters,
// surrogates, private-use and unassigned code points, the line and paragraph separators, and
// U+FFFD (what decoding invalid UTF-8 produces). Tab, newline and CR are the only allowed controls.
const UNREADABLE_CHARACTER_RE = /(?![\t\n\r])[\p{C}\p{Zl}\p{Zp}\uFFFD]/u;
const DIGEST_BYTE_LENGTH = 32;
// One unbroken run of hex, base64, base64url or dotted-token characters and nothing else: an
// unprefixed hash, a base64 digest, a UUID, a JWT-like token. Sentences carry spaces and
// punctuation outside this alphabet.
const TOKEN_SHAPED_RE = /^[A-Za-z0-9+/=_.-]{32,}$/;

export function isOpaqueSignatureMessage(message: string): boolean {
  if (CALLDATA_RE.test(message) || UNREADABLE_CHARACTER_RE.test(message)) return true;
  if (new TextEncoder().encode(message).length === DIGEST_BYTE_LENGTH) return true;
  return TOKEN_SHAPED_RE.test(message.trim());
}

export type UnverifiableReason =
  | "opaque_message"
  | "unrecognized_typed_data"
  | "unsimulated_transaction";

// Upstream exempts only what it can vouch for: readable personal_sign text, a typed-data
// MetaTransaction whose inner call simulated cleanly on a recognized Decentraland contract, and
// a transaction whose preview was produced. This page has neither the simulation nor the
// registry, so everything but readable text stays behind the acknowledgment.
export function unverifiableReason(
  method: AllowedMethod,
  params: unknown[],
): UnverifiableReason | null {
  if (method === "eth_sendTransaction") return "unsimulated_transaction";
  if (method !== "personal_sign") return "unrecognized_typed_data";
  const message = decodeSignatureMessage(params[0]);
  return message === null || isOpaqueSignatureMessage(message) ? "opaque_message" : null;
}
