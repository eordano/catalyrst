import {
  rejectionOutcome,
  unverifiableReason,
  validateAuthRequest,
  type AllowedMethod,
  type RequestRejection,
  type UnverifiableReason,
  namedSender,
} from "./auth-request-params";

export type RecoverResponse = {
  expiration: string;
  code: number;
  method: string;
  params?: unknown[];
  sender?: string;
  challenge?: string;
};

export type ReadyRequest = Omit<RecoverResponse, "method" | "params"> & {
  method: AllowedMethod;
  params: unknown[];
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseRecoverResponse(value: unknown): RecoverResponse | null {
  if (!isRecord(value)) return null;
  const { expiration, code, method, params, sender, challenge } = value;
  if (typeof expiration !== "string" || typeof code !== "number") return null;
  if (typeof method !== "string") return null;
  if (params !== undefined && !Array.isArray(params)) return null;
  if (sender !== undefined && typeof sender !== "string") return null;
  if (challenge !== undefined && typeof challenge !== "string") return null;
  return { expiration, code, method, params, sender, challenge };
}

export type LoadResult =
  | { kind: "ok"; request: RecoverResponse }
  | { kind: "not_found" }
  | { kind: "expired" }
  | { kind: "fulfilled" }
  | { kind: "error"; message: string };

export type RecoveryOutcome =
  | {
      kind: "ready";
      request: ReadyRequest;
      needsValidation: boolean;
      unverifiable: UnverifiableReason;
    }
  | { kind: "not_found" }
  | { kind: "expired" }
  | { kind: "fulfilled" }
  | { kind: "error"; message: string }
  | { kind: "rejected"; rejection: RequestRejection; reported: boolean };

export type OutcomeError = { code: number; message: string };

export type RecoveryDeps = {
  load: (id: string) => Promise<LoadResult>;
  requiresValidation: (id: string) => Promise<boolean>;
  connectedAddress: () => Promise<string | null>;
  postOutcome: (id: string, body: { sender: string; error: OutcomeError }) => Promise<{ ok: boolean }>;
  now?: () => number;
};

export async function recoverAuthRequest(
  id: string,
  deps: RecoveryDeps,
): Promise<RecoveryOutcome> {
  const [result, needsValidation] = await Promise.all([
    deps.load(id),
    deps.requiresValidation(id),
  ]);
  if (result.kind !== "ok") return result;

  const req = result.request;
  const expiresAt = Date.parse(req.expiration);
  if (Number.isFinite(expiresAt) && expiresAt <= (deps.now ?? Date.now)()) {
    return { kind: "expired" };
  }

  const validated = validateAuthRequest(req.method, req.params, req.sender ?? null);
  if (!validated.ok) {
    const { rejection } = validated;
    const sender = (await deps.connectedAddress().catch(() => null)) ?? namedSender(req.sender) ?? "";
    const reported = sender
      ? await deps
          .postOutcome(id, { sender, error: rejectionOutcome(rejection) })
          .then((res) => res.ok, () => false)
      : false;
    return { kind: "rejected", rejection, reported };
  }

  const { method, params } = validated.request;
  return {
    kind: "ready",
    request: { ...req, method, params },
    needsValidation,
    unverifiable: unverifiableReason(method, params),
  };
}
