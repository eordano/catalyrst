import {
  rejectionOutcome,
  unverifiableReason,
  validateAuthRequest,
  type AllowedMethod,
  type RequestRejection,
  type UnverifiableReason,
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
      unverifiable: UnverifiableReason | null;
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

// The recover half of upstream's RequestPage.loadRequest. A request the guards refuse never
// reaches the wallet, so nothing else would answer it and the client would block until it
// expires: the rejection is reported with the wallet's non-prompting account (or the request's
// own sender) as best effort. Expired, fulfilled, missing and network failures stay unreported:
// there is nothing left to answer, or the error view retries. Whether the connected account
// matches the request's sender is only known on approve, so that check lives there.
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
    const sender = (await deps.connectedAddress().catch(() => null)) ?? req.sender ?? "";
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
