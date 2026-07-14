import { z } from "zod";

import { CatalystError, getJSON, postJSON, signedGetJSON } from "../client";
import type { GetOptions } from "../client";
import type { AuthIdentity } from "../../auth/types";
import {
  BanStatusSchema as WireBanStatusSchema,
  UserBanSchema,
  UserWarningSchema,
} from "../generated-schemas/comms";
import { shortAddress as shortAddressCore } from "../format/address";

const ADDR_RE = /^0x[a-fA-F0-9]{40}$/;

export function isAddress(value: string | null | undefined): boolean {
  return typeof value === "string" && ADDR_RE.test(value.trim());
}

export function normalizeAddress(value: string): string {
  return value.trim().toLowerCase();
}

export function shortAddress(value: string): string {
  return shortAddressCore(value.trim());
}

export type DurationPreset = { id: string; label: string; ms: number | null };

export const DURATION_PRESETS: DurationPreset[] = [
  { id: "permanent", label: "Permanent", ms: null },
  { id: "1h", label: "1 hour", ms: 60 * 60 * 1000 },
  { id: "24h", label: "24 hours", ms: 24 * 60 * 60 * 1000 },
  { id: "7d", label: "7 days", ms: 7 * 24 * 60 * 60 * 1000 },
  { id: "30d", label: "30 days", ms: 30 * 24 * 60 * 60 * 1000 },
];

export function durationMsFor(presetId: string | null | undefined): number | null {
  const p = DURATION_PRESETS.find((x) => x.id === presetId);
  return p ? p.ms : null;
}

export type BanPlayerBody = {
  reason: string;
  duration?: number;
  customMessage?: string;
};

export type WarnPlayerBody = { reason: string };

export type FieldErrors = Record<string, string>;

export function validateReason(reason: string): FieldErrors {
  const errors: FieldErrors = {};
  if (reason.trim().length === 0) errors.reason = "Enter a reason for this action.";
  return errors;
}

/**
 * Validation truth is the generated comms schemas (`UserBanSchema`,
 * `UserWarningSchema`, `WireBanStatusSchema` -- the ts-rs images of
 * catalyrst-comms' DTOs). `name` is NOT on that wire: it is a client-side
 * enrichment slot (stories and any profile-joining loader fill it), so it is
 * attached in an explicit post-parse step instead of being folded into the
 * schema.
 */
export { UserBanSchema, UserWarningSchema };
export type WireUserBan = z.infer<typeof UserBanSchema>;
export type UserBan = WireUserBan & { name: string | null };
export type UserWarning = z.infer<typeof UserWarningSchema>;

export const BanStatusSchema = WireBanStatusSchema;
export type BanStatus = { isBanned: boolean; ban: UserBan | null };

/** Post-parse enrichment: lift a `name` the caller may have attached to the
 *  raw row; the comms wire itself never carries one. */
function withName(row: WireUserBan, raw: unknown): UserBan {
  const name = (raw as { name?: unknown } | null)?.name;
  return { ...row, name: typeof name === "string" ? name : null };
}

const envelope = <T extends z.ZodTypeAny>(inner: T) =>
  z.object({ data: inner });

/**
 * The list envelopes require `data`, and the loaders below answer null when it
 * is not there. "This player has no bans" and "we could not read the ban list"
 * are the two answers a moderator most needs to tell apart, and an empty array
 * says the first one.
 */
const BansListEnvelope = envelope(z.array(z.unknown()));
const BanStatusEnvelope = envelope(z.unknown());
const WarningsEnvelope = envelope(z.array(z.unknown()));
const RowEnvelope = envelope(z.unknown());

export function parseBans(rows: unknown[]): UserBan[] {
  const out: UserBan[] = [];
  for (const raw of rows) {
    const r = UserBanSchema.safeParse(raw);
    if (r.success) out.push(withName(r.data, raw));
  }
  return out;
}

export function parseWarnings(rows: unknown[]): UserWarning[] {
  const out: UserWarning[] = [];
  for (const raw of rows) {
    const r = UserWarningSchema.safeParse(raw);
    if (r.success) out.push(r.data);
  }
  return out;
}

export function parseBanStatus(raw: unknown): BanStatus {
  const r = WireBanStatusSchema.safeParse(raw);
  if (!r.success) return { isBanned: false, ban: null };
  const rawBan = (raw as { ban?: unknown } | null)?.ban;
  return {
    isBanned: r.data.isBanned,
    ban: r.data.ban ? withName(r.data.ban, rawBan) : null,
  };
}

export const COMMS_PREFIX = "/comms";

export const ALL_BANS_PATH = "/bans";

export function userBansPath(address: string): string {
  return `/users/${encodeURIComponent(normalizeAddress(address))}/bans`;
}

export function userWarningsPath(address: string): string {
  return `/users/${encodeURIComponent(normalizeAddress(address))}/warnings`;
}

export type ModeratedGetOptions = GetOptions & { identity?: AuthIdentity | null };

async function commsGet<T>(path: string, opts: ModeratedGetOptions): Promise<T> {
  const { identity, ...rest } = opts;
  const url = `${COMMS_PREFIX}${path}`;
  if (!identity) return getJSON<T>(url, rest);
  return signedGetJSON<T>(url, {
    identity,
    signPath: path,
    base: rest.base,
    query: rest.query,
    signal: rest.signal,
  });
}

/** Null when the ban list could not be read -- never an empty list. */
export async function loadActiveBans(
  opts: ModeratedGetOptions = {},
): Promise<UserBan[] | null> {
  const env = await commsGet<unknown>(ALL_BANS_PATH, opts);
  const parsed = BansListEnvelope.safeParse(env);
  return parsed.success ? parseBans(parsed.data.data) : null;
}

export async function loadBanStatus(address: string, opts: GetOptions = {}): Promise<BanStatus> {
  const env = await getJSON<unknown>(`${COMMS_PREFIX}${userBansPath(address)}`, opts);
  const parsed = BanStatusEnvelope.safeParse(env);
  return parsed.success ? parseBanStatus(parsed.data.data) : { isBanned: false, ban: null };
}

/** Null when the warnings list could not be read -- never an empty list. */
export async function loadWarnings(
  address: string,
  opts: ModeratedGetOptions = {},
): Promise<UserWarning[] | null> {
  const env = await commsGet<unknown>(userWarningsPath(address), opts);
  const parsed = WarningsEnvelope.safeParse(env);
  return parsed.success ? parseWarnings(parsed.data.data) : null;
}

export const USER_ACTIONS = ["ban", "unban", "warn"] as const;
export type UserAction = (typeof USER_ACTIONS)[number];

export type ActionFailureReason = "already_banned" | "no_active_ban";

export class UserActionError extends Error {
  readonly reason: ActionFailureReason;
  readonly status: number;
  constructor(reason: ActionFailureReason, address: string) {
    const { status, message } =
      reason === "already_banned"
        ? { status: 409, message: `Player is already banned: ${address}` }
        : { status: 404, message: `No active ban found for player: ${address}` };
    super(message);
    this.name = "UserActionError";
    this.reason = reason;
    this.status = status;
  }
}

export type UserActionResult = {
  action: UserAction;
  address: string;
  ban?: UserBan;
  warning?: UserWarning;
};

export const NOT_CONNECTED_MESSAGE = "Connect your wallet to commit this change.";

export type CommitUserActionArgs = {
  identity: AuthIdentity | null | undefined;
  action: UserAction;
  address: string;
  reason: string;
  durationMs?: number | null;
  customMessage?: string | null;
  base?: string;
  signal?: AbortSignal;
};

function parseRow<S extends z.ZodTypeAny>(raw: unknown, schema: S): z.infer<S> | undefined {
  const env = RowEnvelope.safeParse(raw);
  if (!env.success) return undefined;
  const row = schema.safeParse(env.data.data);
  return row.success ? row.data : undefined;
}

function parseBanRow(raw: unknown): UserBan | undefined {
  const env = RowEnvelope.safeParse(raw);
  if (!env.success) return undefined;
  const row = UserBanSchema.safeParse(env.data.data);
  return row.success ? withName(row.data, env.data.data) : undefined;
}

function commitError(err: unknown, action: UserAction, address: string): unknown {
  if (!(err instanceof CatalystError)) return err;
  if (err.status === 409) return new UserActionError("already_banned", address);
  if (err.status === 404 && action === "unban") {
    return new UserActionError("no_active_ban", address);
  }
  return err;
}

export async function commitUserAction(args: CommitUserActionArgs): Promise<UserActionResult> {
  const { identity, action, reason, durationMs, customMessage, base, signal } = args;
  const address = normalizeAddress(args.address);
  if (!identity) throw new Error(NOT_CONNECTED_MESSAGE);

  try {
    if (action === "unban") {
      const path = userBansPath(address);
      await postJSON<void>(`${COMMS_PREFIX}${path}`, undefined, {
        identity,
        method: "DELETE",
        base,
        signPath: path,
        signal,
      });
      return { action, address };
    }

    if (action === "warn") {
      const path = userWarningsPath(address);
      const body: WarnPlayerBody = { reason };
      const raw = await postJSON<unknown>(`${COMMS_PREFIX}${path}`, body, {
        identity,
        base,
        signPath: path,
        signal,
      });
      return { action, address, warning: parseRow(raw, UserWarningSchema) };
    }

    const path = userBansPath(address);
    const body: BanPlayerBody = { reason };
    if (typeof durationMs === "number" && durationMs > 0) body.duration = durationMs;
    if (customMessage && customMessage.trim()) body.customMessage = customMessage.trim();
    const raw = await postJSON<unknown>(`${COMMS_PREFIX}${path}`, body, {
      identity,
      base,
      signPath: path,
      signal,
    });
    return { action, address, ban: parseBanRow(raw) };
  } catch (err) {
    throw commitError(err, action, address);
  }
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;
/** The view type is exactly the wire row plus the enrichment slot. */
export type _AssertUserBanIsWirePlusName = Assert<
  Mutual<Omit<UserBan, "name">, WireUserBan>
>;
