import type { StoryMeta, Variant } from "./context";
import { getForcedFlags, getRuntimeFlags } from "./flags";

export type Assignment = {
  variant: string;
  flags: Record<string, unknown>;
  experimentKey: string;
};

const SID_COOKIE = "sid";
const SID_MAX_AGE = 60 * 60 * 24 * 365;
const WALLET_COOKIE = "dcl_wallet_verified";
const WALLET_MAX_AGE = 60 * 60 * 24 * 30;
const ADDRESS = /^0x[0-9a-f]{40}$/;

export function parseCookies(header: string | null | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  if (!header) return out;
  for (const part of header.split(";")) {
    const eq = part.indexOf("=");
    if (eq === -1) continue;
    const k = part.slice(0, eq).trim();
    if (!k) continue;
    const v = part.slice(eq + 1).trim();
    try {
      out[k] = decodeURIComponent(v);
    } catch {
      out[k] = v;
    }
  }
  return out;
}

export function readSid(request: Request): string | null {
  const sid = parseCookies(request.headers.get("cookie"))[SID_COOKIE];
  return sid && sid.length > 0 ? sid : null;
}

// The one parent domain sid cookies may be widened to. Widening exists so a
// session minted on any *.catalyst.example.com surface is the same session on every other
//; hosts outside the parent always stay host-scope.
const SID_SHARED_PARENT = "catalyst.example.com";

/** The shared parent domain the request's host sits under, or undefined -- the
 *  only value callers may pass as serializeSidCookie's widening opt-in. */
export function sharedSidDomain(request: Request): string | undefined {
  let hostname: string;
  try {
    hostname = new URL(request.url).hostname;
  } catch {
    return undefined;
  }
  return hostname === SID_SHARED_PARENT ||
    hostname.endsWith(`.${SID_SHARED_PARENT}`)
    ? SID_SHARED_PARENT
    : undefined;
}

/** True when the Cookie header carries `sid` more than once -- a host-scope and
 *  a wide-scope cookie coexisting. The browser's send order for same-named
 *  same-path cookies is not stable across restarts, so which one readSid keeps
 *  can flip between sessions; a response should converge the pair. */
export function hasSplitSidCookie(request: Request): boolean {
  const header = request.headers.get("cookie");
  if (!header) return false;
  let seen = 0;
  for (const part of header.split(";")) {
    const eq = part.indexOf("=");
    if (eq !== -1 && part.slice(0, eq).trim() === SID_COOKIE) seen += 1;
  }
  return seen > 1;
}

export function createSid(): string {
  try {
    if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
      return crypto.randomUUID();
    }
  } catch {
  }
  let s = "";
  for (let i = 0; i < 32; i++) s += Math.floor(Math.random() * 16).toString(16);
  return s;
}

export function ensureSid(request: Request): { sid: string; created: boolean } {
  const existing = readSid(request);
  if (existing) return { sid: existing, created: false };
  return { sid: createSid(), created: true };
}

export function serializeSidCookie(
  sid: string,
  opts?: { domain?: string },
): string {
  return [
    `${SID_COOKIE}=${encodeURIComponent(sid)}`,
    "Path=/",
    `Max-Age=${SID_MAX_AGE}`,
    "SameSite=Lax",
    "HttpOnly",
    // Secure, like the wallet cookie next door: this is a year-lived
    // identifier and without the flag a single plaintext request (a typed
    // URL, an http:// link, a downgrade) sends it in the clear. Every host
    // that serves this app is HTTPS.
    "Secure",
    // Domain widening is opt-in per response: only a caller that knows the
    // request host sits under the shared parent may ask for it, so a session
    // minted on an unrelated host never sets a cookie the browser would
    // reject (a mismatched Domain drops the whole cookie, killing the
    // session).
    ...(opts?.domain ? [`Domain=${opts.domain}`] : []),
  ].join("; ");
}

/** The expiry twin of serializeSidCookie: same attributes, empty value,
 *  Max-Age=0. Emitted beside a wide-scope re-issue to delete the host-scope
 *  cookie of the same name, so exactly one sid survives the response. */
export function expireSidCookie(opts?: { domain?: string }): string {
  return [
    `${SID_COOKIE}=`,
    "Path=/",
    "Max-Age=0",
    "SameSite=Lax",
    "HttpOnly",
    "Secure",
    ...(opts?.domain ? [`Domain=${opts.domain}`] : []),
  ].join("; ");
}

export function readVerifiedWallet(request: Request): string | null {
  const w = parseCookies(request.headers.get("cookie"))[WALLET_COOKIE];
  return w && ADDRESS.test(w) ? w : null;
}

export function serializeVerifiedWalletCookie(address: string): string {
  return [
    `${WALLET_COOKIE}=${encodeURIComponent(address.toLowerCase())}`,
    "Path=/",
    `Max-Age=${WALLET_MAX_AGE}`,
    "SameSite=Lax",
    "HttpOnly",
    "Secure",
  ].join("; ");
}

export function clearVerifiedWalletCookie(): string {
  return [
    `${WALLET_COOKIE}=`,
    "Path=/",
    "Max-Age=0",
    "SameSite=Lax",
    "HttpOnly",
    "Secure",
  ].join("; ");
}

export function cyrb53(str: string, seed = 0): number {
  let h1 = 0xdeadbeef ^ seed;
  let h2 = 0x41c6ce57 ^ seed;
  for (let i = 0; i < str.length; i++) {
    const ch = str.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507);
  h1 ^= Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507);
  h2 ^= Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return 4294967296 * (2097151 & h2) + (h1 >>> 0);
}

export function hashToUnitInterval(input: string): number {
  return cyrb53(input) / 9007199254740992;
}

export function bucket(sid: string, experimentKey: string, variants: Variant[]): Variant {
  if (variants.length === 0) {
    throw new Error("bucket() requires at least one variant");
  }
  if (variants.length === 1) return variants[0];

  const weights = variants.map((v) => (v.weight > 0 ? v.weight : 0));
  let total = weights.reduce((a, b) => a + b, 0);
  const effective = total > 0 ? weights : variants.map(() => 1);
  total = effective.reduce((a, b) => a + b, 0);

  const target = hashToUnitInterval(`${sid}:${experimentKey}`) * total;
  let acc = 0;
  for (let i = 0; i < variants.length; i++) {
    acc += effective[i];
    if (target < acc) return variants[i];
  }
  return variants[variants.length - 1];
}

function buildAssignment(
  experimentKey: string,
  variant: Variant | undefined,
  story: StoryMeta,
  overrideFlags?: Record<string, unknown>,
): Assignment {
  const resolved = variant ?? story.experiment.variants[0];
  return {
    variant: resolved.id,
    experimentKey,
    flags: { ...resolved.flags, ...(overrideFlags ?? {}) },
  };
}

function findVariant(story: StoryMeta, id: string | undefined): Variant | undefined {
  if (!id) return undefined;
  return story.experiment.variants.find((v) => v.id === id);
}

export async function resolveAssignment(
  subject: Request | string,
  story: StoryMeta,
  opts: { user?: string } = {},
): Promise<Assignment> {
  const sid = typeof subject === "string" ? subject : ensureSid(subject).sid;
  const user =
    opts.user ??
    (typeof subject === "string" ? sid : readVerifiedWallet(subject) ?? sid);
  const experimentKey = story.experiment.key;
  const defaultVariant = story.experiment.variants[0];

  try {
    const [forcedFlags, flag] = await Promise.all([
      getForcedFlags({ user }),
      getRuntimeFlags(experimentKey, { user }),
    ]);
    const dashFlag = forcedFlags?.[experimentKey];
    const dashForced = dashFlag?.overridden === true;
    if (dashForced) {
      if (!dashFlag.value) {
        const pinned = findVariant(story, dashFlag.variant) ?? defaultVariant;
        return buildAssignment(experimentKey, pinned, story, flag?.flags);
      }
      if (dashFlag.variant) {
        const pinned = findVariant(story, dashFlag.variant);
        if (pinned) return buildAssignment(experimentKey, pinned, story, flag?.flags);
        return {
          variant: dashFlag.variant,
          experimentKey,
          flags: { ...(flag?.flags ?? {}) },
        };
      }
    }
    if (flag) {
      if (flag.killed && !dashForced) {
        const pinned = findVariant(story, flag.variant) ?? defaultVariant;
        return buildAssignment(experimentKey, pinned, story, flag.flags);
      }
      if (flag.variant) {
        const forced = findVariant(story, flag.variant);
        if (forced) return buildAssignment(experimentKey, forced, story, flag.flags);
        return {
          variant: flag.variant,
          experimentKey,
          flags: { ...(flag.flags ?? {}) },
        };
      }
      if (flag.flags) {
        const hashed = bucket(sid, experimentKey, story.experiment.variants);
        return buildAssignment(experimentKey, hashed, story, flag.flags);
      }
    }
  } catch {
  }

  const hashed = bucket(sid, experimentKey, story.experiment.variants);
  return buildAssignment(experimentKey, hashed, story);
}
