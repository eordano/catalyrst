import { z } from "zod";

import { CatalystError, getJSON } from "../client";
import type { GetOptions } from "../client";
import { ensSubdomain, formatMana, isEnsBuyable, type EnsResult } from "./index";
import { parseEnsResults, parseMarketEnvelope } from "./schema";
import {
  AssetsHttpResponseSchema,
  ProfileNameSchema,
} from "../generated-schemas/market";
import { warnInvalid } from "../warn";

export type OwnedName = {
  name: string;
  contractAddress: string;
  tokenId: string;
  price: string | null;
};

export function parseOwnedName(raw: unknown): OwnedName | null {
  const r = ProfileNameSchema.safeParse(raw);
  if (!r.success) {
    warnInvalid("OwnedName", r.error.issues);
    return null;
  }
  const n = r.data;
  return {
    name: n.name,
    contractAddress: n.contractAddress,
    tokenId: n.tokenId,
    price: n.price ?? null,
  };
}

const NamesEnvelopeSchema = AssetsHttpResponseSchema(z.unknown());

export type OwnedNamesPage = {
  elements: OwnedName[];
  total: number;
};

export const NAME_ECONOMICS = {
  priceWei: "100000000000000000000",
  priceMana: "100",
  network: "ETHEREUM" as const,
  chainId: 1,
  registrarContractAddress: "0x2a187453064356c898cae034eaed119e1663acb8",
  creditCompatible: true,
  maxNameSize: 15,
  minNameSize: 2,
} as const;

export const NAME_REGEX = /^[a-zA-Z0-9]{2,15}$/;

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

export async function fetchOwnedNames(
  address: string,
  opts: GetOptions = {},
): Promise<OwnedNamesPage> {
  const path = `/market/v1/users/${encodeURIComponent(normalizeAddress(address))}/names`;
  const env = await getJSON<unknown>(path, {
    ...opts,
    query: { pageNum: 1, pageSize: 100 },
  });
  const parsed = NamesEnvelopeSchema.safeParse(env);
  if (!parsed.success) {
    warnInvalid("NamesEnvelope", parsed.error.issues);
    throw new CatalystError("names response did not match the paged-names shape", path);
  }
  const elements: OwnedName[] = [];
  for (const raw of parsed.data.data.elements) {
    const r = parseOwnedName(raw);
    if (r) elements.push(r);
  }
  return { elements, total: parsed.data.data.total };
}

export type NameClassification =
  | { kind: "idle" }
  | { kind: "invalid"; message: string; warn?: boolean }
  | { kind: "taken" }
  | { kind: "available" };

export function classifyName(raw: string, taken: ReadonlySet<string>): NameClassification {
  const name = raw.trim();
  if (name === "") return { kind: "idle" };
  if (/\s/.test(name)) return { kind: "invalid", message: "NAMEs can't contain spaces." };
  if (name.length < NAME_ECONOMICS.minNameSize)
    return {
      kind: "invalid",
      warn: true,
      message: "NAME too short: NAMEs must be at least 2 characters long.",
    };
  if (name.length > NAME_ECONOMICS.maxNameSize)
    return { kind: "invalid", message: "NAMEs can contain up to 15 characters." };
  if (!NAME_REGEX.test(name))
    return { kind: "invalid", message: "NAMEs can only contain alphanumeric characters." };
  if (taken.has(name.toLowerCase())) return { kind: "taken" };
  return { kind: "available" };
}

export type NameAvailability =
  | { kind: "claimable"; name: string }
  | {
      kind: "listed";
      name: string;
      contractAddress: string;
      tokenId: string;
      priceWei: string;
      priceMana: string;
    }
  | { kind: "taken"; name: string };

export function ensOrderExpired(
  expiresAt: number | null | undefined,
  now = Date.now(),
): boolean {
  if (!expiresAt) return false;
  const ms = expiresAt > 1e12 ? expiresAt : expiresAt * 1000;
  return ms <= now;
}

export function ensAvailability(
  name: string,
  results: EnsResult[],
  fallbackTokenId: string,
  now = Date.now(),
): NameAvailability {
  const row = results[0];
  if (!row) return { kind: "claimable", name };
  const minted = ensSubdomain(row);
  const order = row.order;
  if (isEnsBuyable(row) && order?.price && !ensOrderExpired(order.expiresAt, now)) {
    return {
      kind: "listed",
      name: minted,
      contractAddress: row.nft.contractAddress,
      tokenId: row.nft.tokenId ?? fallbackTokenId,
      priceWei: order.price,
      priceMana: formatMana(order.price) ?? "0",
    };
  }
  return { kind: "taken", name: minted };
}

export async function ensNameTokenId(name: string): Promise<string> {
  const { keccak256, toBytes } = await import("viem");
  return BigInt(keccak256(toBytes(name.trim().toLowerCase()))).toString();
}

export async function checkNameAvailability(
  name: string,
  opts: GetOptions = {},
): Promise<NameAvailability> {
  const trimmed = name.trim();
  const tokenId = await ensNameTokenId(trimmed);
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/nfts", {
      ...opts,
      query: {
        contractAddress: NAME_ECONOMICS.registrarContractAddress,
        tokenId,
        first: 1,
      },
    }),
  );
  const rows = parseEnsResults(env.data);
  // A minted-name row we could not validate must not read as "claimable":
  // that answer is what a buyer acts on.
  if (env.data.length > 0 && rows.length === 0) {
    throw new CatalystError(
      "name lookup rows did not match the nft-result shape",
      "/market/v1/nfts",
    );
  }
  return ensAvailability(trimmed, rows, tokenId);
}
