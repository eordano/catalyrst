import { z } from "zod";

import { catalystBase } from "../client";
import {
  SuspendResultSchema,
  type SuspendResult,
} from "./community-moderation";

const InputSchema = z.object({
  communityId: z.string().min(1),
  decision: z.enum(["suspend", "unsuspend"]),
  reason: z.string().optional(),
});

const ErrorMessageSchema = z.object({ message: z.string() });

export type SubmitSuspensionResult =
  | { ok: true; result: SuspendResult }
  | { ok: false; status: number; error: string };

function communitiesBase(): string {
  const direct =
    typeof process !== "undefined" ? process.env?.COMMUNITIES_URL : undefined;
  return (direct || catalystBase()).replace(/\/$/, "");
}

function adminToken(): string | undefined {
  const t =
    typeof process !== "undefined" ? process.env?.API_ADMIN_TOKEN : undefined;
  return t && t.length > 0 ? t : undefined;
}

export async function submitSuspension(
  raw: unknown,
  opts: { signal?: AbortSignal } = {},
): Promise<SubmitSuspensionResult> {
  const parsed = InputSchema.safeParse(raw);
  if (!parsed.success) {
    return { ok: false, status: 400, error: "Invalid moderation request." };
  }
  const { communityId, decision, reason } = parsed.data;

  const token = adminToken();
  if (!token) {
    return {
      ok: false,
      status: 503,
      error:
        "Community moderation is not configured on this node (admin token unset).",
    };
  }

  const verb = decision === "suspend" ? "suspend" : "unsuspend";
  const url = `${communitiesBase()}/v1/admin/communities/${encodeURIComponent(
    communityId,
  )}/${verb}`;
  const body =
    decision === "suspend" && reason && reason.trim()
      ? JSON.stringify({ reason: reason.trim() })
      : undefined;

  let res: Response;
  try {
    res = await fetch(url, {
      method: "POST",
      headers: {
        accept: "application/json",
        authorization: `Bearer ${token}`,
        ...(body ? { "content-type": "application/json" } : {}),
      },
      body,
      signal: opts.signal,
      cache: "no-store",
    });
  } catch (err) {
    return {
      ok: false,
      status: 502,
      error: `Moderation backend unreachable: ${
        (err as Error)?.message ?? "network error"
      }`,
    };
  }

  let payload: unknown = null;
  try {
    payload = await res.json();
  } catch {
  }

  if (!res.ok) {
    const parsed = ErrorMessageSchema.safeParse(payload);
    const msg = parsed.success
      ? parsed.data.message
      : `Moderation backend returned HTTP ${res.status}.`;
    return { ok: false, status: res.status, error: msg };
  }

  const result = SuspendResultSchema.safeParse(payload);
  if (!result.success) {
    return {
      ok: false,
      status: 502,
      error: "Moderation backend returned an unexpected response.",
    };
  }
  return { ok: true, result: result.data };
}
