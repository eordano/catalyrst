import { z } from "zod";

export type VpDistribution = {
  total: number;
  own: number;
  delegated: number;
  mana: number;
  wMana: number;
  names: number;
  l1Wearables: number;
  land: number;
  estate: number;
  rental: number;
};

export const DEFAULT_SNAPSHOT_HUB = "https://hub.snapshot.org";
export const SNAPSHOT_HUB_ENV = "SNAPSHOT_API";

const STRATEGY = {
  wrappedMana: 0,
  land: 1,
  estate: 2,
  names: 3,
  delegation: 4,
  l1Wearables: 5,
  rental: 6,
  manaEth: 7,
  manaPolygon: 8,
} as const;

type Env = Record<string, string | undefined>;

function processEnv(): Env {
  return typeof process !== "undefined" && process.env ? process.env : {};
}

export function snapshotHubUrl(override?: string, env: Env = processEnv()): string {
  const base = override ?? env[SNAPSHOT_HUB_ENV] ?? DEFAULT_SNAPSHOT_HUB;
  return base.trim().replace(/\/$/, "");
}

const VpEntrySchema = z
  .object({ vp: z.number().nullish(), vp_by_strategy: z.array(z.number()).nullish() })
  .nullable();

const VpResponseSchema = z.object({
  data: z.record(z.string(), VpEntrySchema).nullish(),
  errors: z.array(z.object({ message: z.string().nullish() })).nullish(),
});

function at(values: number[], index: number): number {
  const value = values[index];
  return Number.isFinite(value) ? value : 0;
}

export function toVpDistribution(total: number, byStrategy: number[]): VpDistribution {
  const delegated = at(byStrategy, STRATEGY.delegation);
  return {
    total,
    own: Math.max(0, total - delegated),
    delegated,
    mana: at(byStrategy, STRATEGY.manaEth) + at(byStrategy, STRATEGY.manaPolygon),
    wMana: at(byStrategy, STRATEGY.wrappedMana),
    names: at(byStrategy, STRATEGY.names),
    l1Wearables: at(byStrategy, STRATEGY.l1Wearables),
    land: at(byStrategy, STRATEGY.land),
    estate: at(byStrategy, STRATEGY.estate),
    rental: at(byStrategy, STRATEGY.rental),
  };
}

function buildQuery(count: number): string {
  const fields = Array.from(
    { length: count },
    (_, i) => `v${i}: vp(voter: $a${i}, space: $space) { vp vp_by_strategy }`,
  ).join("\n    ");
  const params = Array.from({ length: count }, (_, i) => `$a${i}: String!`).join(", ");
  return `query vpBatch($space: String!, ${params}) {\n    ${fields}\n  }`;
}

const BATCH = 5;

async function fetchBatch(args: {
  hubUrl: string;
  space: string;
  addresses: string[];
  fetchImpl: typeof fetch;
  signal?: AbortSignal;
}): Promise<Map<string, VpDistribution>> {
  const variables: Record<string, string> = { space: args.space };
  args.addresses.forEach((address, i) => {
    variables[`a${i}`] = address;
  });

  const res = await args.fetchImpl(`${args.hubUrl}/graphql`, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({ query: buildQuery(args.addresses.length), variables }),
    signal: args.signal,
  });
  const text = await res.text();
  let payload: unknown;
  try {
    payload = JSON.parse(text);
  } catch {
    throw new Error(`snapshot hub returned ${res.status} with a non-JSON body`);
  }

  const body = VpResponseSchema.parse(payload);
  const data = body.data;
  if (!data) {
    const reported = body.errors?.[0]?.message;
    const reason = reported ?? (res.ok ? "no data" : `HTTP ${res.status}`);
    throw new Error(`snapshot voting power query failed: ${reason}`);
  }

  const out = new Map<string, VpDistribution>();
  args.addresses.forEach((address, i) => {
    const entry = data[`v${i}`];
    if (!entry) return;
    const total = entry.vp ?? 0;
    out.set(address.toLowerCase(), toVpDistribution(total, entry.vp_by_strategy ?? []));
  });
  return out;
}

export const VP_CACHE_TTL_MS = 5 * 60_000;

const cache = new Map<string, { at: number; vp: VpDistribution }>();

export function clearVpCache(): void {
  cache.clear();
}

export async function fetchVpDistributions(args: {
  space: string;
  addresses: string[];
  hubUrl?: string;
  ttlMs?: number;
  now?: number;
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
}): Promise<Map<string, VpDistribution>> {
  const unique = [...new Set(args.addresses.map((a) => a.trim().toLowerCase()))].filter(
    (a) => /^0x[0-9a-f]{40}$/.test(a),
  );
  const out = new Map<string, VpDistribution>();
  if (unique.length === 0) return out;

  const ttlMs = args.ttlMs ?? VP_CACHE_TTL_MS;
  const now = args.now ?? Date.now();
  const missing: string[] = [];
  for (const address of unique) {
    const hit = cache.get(`${args.space}|${address}`);
    if (hit && now - hit.at < ttlMs) out.set(address, hit.vp);
    else missing.push(address);
  }
  if (missing.length === 0) return out;

  const hubUrl = snapshotHubUrl(args.hubUrl);
  const fetchImpl = args.fetchImpl ?? fetch;
  const chunks: string[][] = [];
  for (let i = 0; i < missing.length; i += BATCH) chunks.push(missing.slice(i, i + BATCH));

  const results = await Promise.all(
    chunks.map((addresses) =>
      fetchBatch({ hubUrl, space: args.space, addresses, fetchImpl, signal: args.signal }),
    ),
  );
  for (const chunk of results) {
    for (const [address, vp] of chunk) {
      out.set(address, vp);
      if (ttlMs > 0) cache.set(`${args.space}|${address}`, { at: now, vp });
    }
  }
  return out;
}
