import type { GetOptions } from "../client";
import { shortAddress } from "../format/address";
import { MembersEnvelopeSchema } from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";

export type CommitteeMember = {
  name: string;
  address: string;
  addressShort: string;
  hue: number;
};

export type Committee = {
  name: string;
  description: string;
  members: CommitteeMember[];
};

export type MonthlyDetail = { name: string; value: number; description: string };
export type MonthlyTotal = { total: number; previous: number; details: MonthlyDetail[] };

export type TransparencyData = {
  source: "live" | "empty" | "error";
  reason: string | null;
  committees: Committee[];
};

const ROLE_META: Record<string, { name: string; description: string; order: number }> = {
  committee: {
    name: "DAO Committee",
    description:
      "The DAO Committee holds the multisig keys that enact proposals passed by the community.",
    order: 0,
  },
  council: {
    name: "DAO Council",
    description:
      "The DAO Council reviews grant requests, enacts passed proposals, and represents the community in operational matters.",
    order: 1,
  },
};

function hueFrom(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return Math.abs(h);
}

function titleCase(s: string): string {
  return s
    .split(/[_\s-]+/)
    .map((w) => (w ? w.charAt(0).toUpperCase() + w.slice(1) : ""))
    .join(" ");
}

export async function loadTransparencyData(
  opts: GetOptions = {},
): Promise<TransparencyData> {
  const base = governanceApiBase(opts.base);
  const doFetch = opts.fetchImpl ?? fetch;
  const url = `${base}/members?limit=200`;

  try {
    const res = await doFetch(url, {
      signal: opts.signal,
      headers: { accept: "application/json" },
    });
    if (!res.ok) {
      return {
        source: "error",
        reason: `/members returned HTTP ${res.status}`,
        committees: [],
      };
    }
    const parsed = MembersEnvelopeSchema.safeParse((await res.json()) as unknown);
    if (!parsed.success) {
      return {
        source: "error",
        reason: "/members returned an unrecognised payload",
        committees: [],
      };
    }

    const byRole = new Map<string, CommitteeMember[]>();
    for (const row of parsed.data.data) {
      const addr = row.address.toLowerCase();
      const member: CommitteeMember = {
        name: shortAddress(addr),
        address: addr,
        addressShort: shortAddress(addr),
        hue: hueFrom(addr),
      };
      const list = byRole.get(row.role) ?? [];
      list.push(member);
      byRole.set(row.role, list);
    }

    const committees: Committee[] = [...byRole.entries()]
      .map(([role, members]) => {
        const meta = ROLE_META[role] ?? {
          name: titleCase(role),
          description: "",
          order: 99,
        };
        return { name: meta.name, description: meta.description, members, order: meta.order };
      })
      .sort((a, b) => a.order - b.order)
      .map(({ name, description, members }) => ({ name, description, members }));

    if (committees.length === 0) {
      return { source: "empty", reason: null, committees: [] };
    }
    return { source: "live", reason: null, committees };
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    return {
      source: "error",
      reason: `/members unreachable: ${detail}`,
      committees: [],
    };
  }
}
