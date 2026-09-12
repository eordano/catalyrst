
export const RPC_METHOD_NOT_SUPPORTED = -32601;
export const RPC_INVALID_PARAMS = -32602;
export const RPC_USER_REJECTED = -32003;

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

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/i;
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

export function decodeSignatureMessage(value: unknown): string | null {
  if (typeof value !== "string") return null;
  return decodeHexMessage(value) ?? value;
}

const EPHEMERAL_ADDRESS_OFFSET = "Ephemeral address: ".length;
const EXPIRATION_OFFSET = "Expiration: ".length;

function isEphemeralText(value: string): boolean {
  const lines = value.replace(/\r/g, "").split("\n");
  const addressLine = lines[1];
  const expirationLine = lines[2];
  if (addressLine === undefined || expirationLine === undefined) return false;
  const address = addressLine.slice(EPHEMERAL_ADDRESS_OFFSET);
  const expiration = Date.parse(expirationLine.slice(EXPIRATION_OFFSET));
  return ADDRESS_RE.test(address) && !Number.isNaN(expiration);
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseTypedData(param: unknown): unknown {
  if (typeof param !== "string") return param;
  try {
    return JSON.parse(param);
  } catch {
    return null;
  }
}

function hasPrimaryType(typedData: unknown): boolean {
  return (
    typeof typedData === "object" &&
    typedData !== null &&
    typeof (typedData as { primaryType?: unknown }).primaryType === "string"
  );
}

const EIP712_DOMAIN_FIELD_TYPES: ReadonlyMap<string, string> = new Map([
  ["name", "string"],
  ["version", "string"],
  ["chainId", "uint256"],
  ["verifyingContract", "address"],
  ["salt", "bytes32"],
]);

const META_TRANSACTION_PRIMARY_TYPE = "MetaTransaction";

export function metaTransactionDomainProblem(typedData: unknown): string | null {
  if (!isRecord(typedData) || typedData.primaryType !== META_TRANSACTION_PRIMARY_TYPE) return null;
  const { types, domain, message } = typedData;
  if (!isRecord(types) || !isRecord(domain) || !isRecord(message)) {
    return "the MetaTransaction is missing its types, domain or message";
  }
  const domainKeys = Object.keys(domain);
  if (domainKeys.some((key) => !EIP712_DOMAIN_FIELD_TYPES.has(key))) {
    return "the MetaTransaction domain has a field EIP-712 does not define";
  }
  const domainType = types.EIP712Domain;
  if (domainType === undefined) return "the MetaTransaction does not declare its domain struct";
  const declaresDomainExactly =
    Array.isArray(domainType) &&
    domainType.length === domainKeys.length &&
    new Set(domainType.map((field) => (isRecord(field) ? field.name : undefined))).size ===
      domainKeys.length &&
    domainType.every(
      (field) =>
        isRecord(field) &&
        typeof field.name === "string" &&
        domainKeys.includes(field.name) &&
        field.type === EIP712_DOMAIN_FIELD_TYPES.get(field.name),
    );
  return declaresDomainExactly
    ? null
    : "the MetaTransaction domain type does not match the domain fields";
}

const JSON_NUMBER_LEXEME = /-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/y;
const INTEGER_LEXEME = /^-?(?:0|[1-9]\d*)$/;

function isExactInteger(lexeme: string): boolean {
  if (!INTEGER_LEXEME.test(lexeme)) return false;
  const value = Number(lexeme);
  return Number.isFinite(value) && BigInt(lexeme) === BigInt(value);
}

function hasOnlyExactNumbers(json: string): boolean {
  let inString = false;
  for (let index = 0; index < json.length; index += 1) {
    const char = json[index];
    if (inString) {
      if (char === "\\") index += 1;
      else if (char === '"') inString = false;
      continue;
    }
    if (char === '"') {
      inString = true;
      continue;
    }
    if (char === "-" || (char >= "0" && char <= "9")) {
      JSON_NUMBER_LEXEME.lastIndex = index;
      const match = JSON_NUMBER_LEXEME.exec(json);
      if (!match || !isExactInteger(match[0])) return false;
      index += match[0].length - 1;
    }
  }
  return true;
}

const MAX_TYPED_DATA_DEPTH = 64;

function hasExcessiveTypedDataDepth(value: unknown, depth = 0): boolean {
  if (value === null || typeof value !== "object") return false;
  if (depth >= MAX_TYPED_DATA_DEPTH) return true;
  return Object.values(value).some((child) => hasExcessiveTypedDataDepth(child, depth + 1));
}

export const MAX_SIGNATURE_PAYLOAD_CHARS = 96 * 1024;

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
    if (firstIsSigner) return "expected [message, signer]";
    return first.length > MAX_SIGNATURE_PAYLOAD_CHARS ? "the message is too large to review" : null;
  }

  if (typeof second === "string" && second.length > MAX_SIGNATURE_PAYLOAD_CHARS) {
    return "the typed data is too large to review";
  }
  const typedData = parseTypedData(second);
  if (!isSigner(first)) return "expected [signer, typedData]";
  if (!hasPrimaryType(typedData)) return "typed data must declare a primaryType";
  if (typeof second === "string" && !hasOnlyExactNumbers(second)) {
    return "the typed data contains a number that cannot be represented exactly";
  }
  if (hasExcessiveTypedDataDepth(typedData)) {
    return "the typed data is too deeply nested to review";
  }
  if (typeof second !== "string" && JSON.stringify(second).length > MAX_SIGNATURE_PAYLOAD_CHARS) {
    return "the typed data is too large to review";
  }
  return null;
}

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

export function toHexQuantity(value: unknown): string {
  if (typeof value !== "string" || !QUANTITY_RE.test(value)) {
    throw new Error(
      `Transaction "value" must be a hex or decimal quantity, received ${describeType(value)}: ${JSON.stringify(value ?? null)}`,
    );
  }
  return `0x${BigInt(value).toString(16)}`;
}

export type TransactionParams = { to: string; data: string; value: string };

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
  const data = source.data ?? "0x";
  if (typeof data !== "string") {
    throw new Error(
      `Transaction "data" must be hex-encoded bytes, received ${describeType(data)}: ${JSON.stringify(data)}`,
    );
  }
  return [
    { to: to.toLowerCase(), data: data.toLowerCase(), value: toHexQuantity(source.value ?? "0x0") },
  ];
}

export type TransactionPreview = { shown: TransactionParams; dropped: string[] };

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

const UNREADABLE_CHARACTER_RE = /(?![\t\n\r])[\p{C}\p{Zl}\p{Zp}\uFFFD]/u;
const DIGEST_BYTE_LENGTH = 32;
const TOKEN_SHAPED_RE = /^[A-Za-z0-9+/=_.-]{32,}$/;

const HEX_STRING_RE = /^0x([0-9a-fA-F]{2})*$/i;

export function isOpaqueSignatureMessage(message: string): boolean {
  if (HEX_STRING_RE.test(message) || UNREADABLE_CHARACTER_RE.test(message)) return true;
  if (new TextEncoder().encode(message).length === DIGEST_BYTE_LENGTH) return true;
  return TOKEN_SHAPED_RE.test(message.trim());
}

export type UnverifiableReason =
  | "opaque_message"
  | "unverified_message"
  | "unrecognized_typed_data"
  | "unsimulated_transaction";

export function unverifiableReason(
  method: AllowedMethod,
  params: unknown[],
): UnverifiableReason {
  if (method === "eth_sendTransaction") return "unsimulated_transaction";
  if (method !== "personal_sign") return "unrecognized_typed_data";
  const message = decodeSignatureMessage(params[0]);
  return message === null || isOpaqueSignatureMessage(message)
    ? "opaque_message"
    : "unverified_message";
}

export function isSameAccount(a: string | null | undefined, b: string | null | undefined): boolean {
  if (typeof a !== "string" || typeof b !== "string") return false;
  const left = a.trim().toLowerCase();
  return left.length > 0 && left === b.trim().toLowerCase();
}

export function namedSender(sender: string | null | undefined): string | null {
  return typeof sender === "string" && sender.trim().length > 0 ? sender : null;
}

export function denialSender(
  request: { sender?: string | null } | null | undefined,
  connected: string | null,
): string {
  return namedSender(request?.sender) ?? connected ?? "";
}

export type ApprovalGates = {
  isSigning: boolean;
  mustValidate: boolean;
  acknowledged: boolean;
  unverifiable: UnverifiableReason | null;
  effectsAcknowledged: boolean;
};

export function approveBlocked(gates: ApprovalGates): boolean {
  if (gates.isSigning) return true;
  if (gates.mustValidate && !gates.acknowledged) return true;
  return gates.unverifiable !== null && !gates.effectsAcknowledged;
}
