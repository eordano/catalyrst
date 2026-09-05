import { z } from "zod";

import { getJSON, postJSON } from "../client";
import { manaToWei } from "./money";
import type { GetOptions } from "../client";
import type { ProfileWearable as RsProfileWearable } from "@ui/generated/catalyst/market/ProfileWearable";
import { requireAssetsEnvelope, thumbnailFromUrn } from "./account";
import {
  assertListable,
  buildListingTrade,
  offchainMarketplaceFor,
  tradeTypedData,
  type TradeTypedData,
} from "./trade";
import {
  ensureApprovalForAll,
  readChainId,
  readSignatureIndexes,
  type WaitOptions,
} from "./sell-chain";
import type { Eip1193Provider } from "../../auth/wallet";
import type { AuthIdentity } from "../../auth/types";
import { warnInvalid } from "../warn";

const nullableStr = z.string().nullish().transform((v) => v ?? null);

export const OwnedAssetSchema = z.object({
  id: z.string(),
  contractAddress: z.string(),
  tokenId: z.string(),
  itemId: nullableStr,
  issuedId: nullableStr,
  activeOrderId: nullableStr,
  owner: nullableStr,
  name: nullableStr,
  category: nullableStr,
  rarity: nullableStr,
  network: nullableStr,
  chainId: z.number().nullish().transform((v) => v ?? null),
  image: nullableStr,
  urn: nullableStr,
  bodyShape: nullableStr,
  isOnSale: z.boolean().nullish().transform((v) => v ?? null),
  status: z.string().nullish(),
  unlockAt: z.number().nullish(),
  lease: z.unknown().optional(),
  usageGrant: z.unknown().optional(),
});

export type OwnedAsset = z.infer<typeof OwnedAssetSchema>;

export function parseOwnedAsset(raw: unknown): OwnedAsset | null {
  const r = OwnedAssetSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("OwnedAsset", r.error.issues);
  return null;
}

export function parseOwnedAssets(raw: unknown[]): OwnedAsset[] {
  const out: OwnedAsset[] = [];
  for (const row of raw ?? []) {
    const asset = parseOwnedAsset(row);
    if (asset) out.push(asset);
  }
  return out;
}

export function ownedAssetImage(asset: OwnedAsset): string | undefined {
  return asset.image ?? thumbnailFromUrn(asset.urn);
}

export async function fetchOwnedWearables(
  address: string,
  params: { first?: number; skip?: number } = {},
  opts: GetOptions = {},
): Promise<OwnedAsset[]> {
  const path = `/market/v1/users/${encodeURIComponent(address)}/wearables`;
  const page = requireAssetsEnvelope(
    await getJSON<unknown>(path, {
      ...opts,
      query: { first: params.first, skip: params.skip },
    }),
    path,
  );
  return parseOwnedAssets(page.elements);
}

export type ListingStatus = "open" | "sold" | "cancelled";

export type SellOrder = {
  id: string;
  marketplaceAddress: string;
  contractAddress: string;
  tokenId: string;
  owner: string;
  buyer: string | null;
  price: string;
  status: ListingStatus;
  expiresAt: number;
  createdAt: number;
  updatedAt: number;
  network: string;
  chainId: number | null;
  issuedId: string;
};

export type BuildSellOrderInput = {
  asset: Pick<
    OwnedAsset,
    "contractAddress" | "tokenId" | "issuedId" | "owner" | "network" | "chainId"
  >;
  priceMana: number;
  expiresAt: number;
};

export function buildSellOrder(input: BuildSellOrderInput): SellOrder {
  const { asset, priceMana, expiresAt } = input;
  const now = Date.now();
  const chainId = asset.chainId ?? 137;
  const market = offchainMarketplaceFor(chainId);
  return {
    id: "",
    marketplaceAddress: market.address,
    contractAddress: asset.contractAddress,
    tokenId: asset.tokenId,
    owner: asset.owner ?? "",
    buyer: null,
    price: manaToWei(priceMana),
    status: "open",
    expiresAt,
    createdAt: now,
    updatedAt: now,
    network: asset.network ?? market.network,
    chainId,
    issuedId: asset.issuedId ?? asset.tokenId,
  };
}

export type CreateOrderResult = {
  order: SellOrder;
  approvalTxHash: string | null;
};

export type CreateOrderFn = (args: {
  order: SellOrder;
  signal?: AbortSignal;
}) => Promise<CreateOrderResult>;

export const failClosedCreate: CreateOrderFn = async () => {
  throw new Error("listing unavailable: order relayer not configured");
};

export type SignTradeFn = (
  typedData: TradeTypedData,
  address: string,
) => Promise<string>;

export type CreateOrderDeps = {
  identity: AuthIdentity | null;
  provider: Eip1193Provider | null;
  address: string | null;
  base?: string;
  signTrade?: SignTradeFn;
  approval?: WaitOptions;
  now?: () => number;
};

const CreatedTradeSchema = z.object({
  data: z.object({ id: z.string().min(1) }),
});

const SIGNATURE_HEX_LENGTH = 132;

// Declared verbatim: catalyrst-market compares these unfolded, so a re-spelled
// value is refused rather than normalised (upstream marketplace-server #393).
export const TRADE_AUTH_METADATA = {
  signer: "dcl:marketplace",
  intent: "dcl:create-trade",
} as const;

const signWithWallet: SignTradeFn = async (typedData, address) => {
  const { signTypedData } = await import("../../auth/typed-data");
  return signTypedData(typedData, address);
};

export function buildCreateOrder(deps: CreateOrderDeps): CreateOrderFn {
  return async ({ order, signal }): Promise<CreateOrderResult> => {
    const { identity, provider, address } = deps;
    if (!identity) throw new Error("listing unavailable: sign in first");
    if (!provider || !address) {
      throw new Error("listing unavailable: connect a browser wallet first");
    }

    const seller = address.trim().toLowerCase();
    if (identity.signer.toLowerCase() !== seller) {
      throw new Error(
        "listing unavailable: the connected wallet is not the signed-in account",
      );
    }
    if (order.owner && order.owner.toLowerCase() !== seller) {
      throw new Error(
        "listing unavailable: the connected wallet does not own this item",
      );
    }
    if (order.chainId === null) {
      throw new Error("listing unavailable: this item has no known chain");
    }

    const market = offchainMarketplaceFor(order.chainId);
    const walletChainId = await readChainId(provider);
    if (walletChainId !== order.chainId) {
      throw new Error(
        `listing unavailable: switch the wallet to ${market.networkLabel} (chain ${order.chainId}); it is on chain ${walletChainId}`,
      );
    }

    const now = deps.now?.() ?? Date.now();
    assertListable({
      tokenId: order.tokenId,
      priceWei: order.price,
      expiresAt: order.expiresAt,
      effectiveAt: now,
    });

    const { txHash } = await ensureApprovalForAll(
      provider,
      {
        contractAddress: order.contractAddress,
        owner: seller,
        operator: market.address,
      },
      { ...deps.approval, signal },
    );

    const indexes = await readSignatureIndexes(provider, market.address, seller);
    const trade = buildListingTrade({
      signer: seller,
      chainId: order.chainId,
      contractAddress: order.contractAddress,
      tokenId: order.tokenId,
      priceWei: order.price,
      expiresAt: order.expiresAt,
      effectiveAt: now,
      ...indexes,
    });

    const sign = deps.signTrade ?? signWithWallet;
    const signature = await sign(tradeTypedData(trade), seller);
    if (
      typeof signature !== "string" ||
      !signature.startsWith("0x") ||
      signature.length < SIGNATURE_HEX_LENGTH
    ) {
      throw new Error(
        "listing unavailable: the wallet returned an invalid trade signature",
      );
    }

    const created = CreatedTradeSchema.safeParse(
      await postJSON<unknown>(
        "/market/v1/trades",
        { ...trade, signature },
        {
          identity,
          signal,
          base: deps.base,
          metadata: { ...TRADE_AUTH_METADATA },
        },
      ),
    );
    if (!created.success) {
      throw new Error(
        "listing unavailable: the marketplace did not return a trade id",
      );
    }

    return {
      order: {
        ...order,
        id: created.data.data.id,
        marketplaceAddress: market.address,
        createdAt: now,
        updatedAt: now,
      },
      approvalTxHash: txHash,
    };
  };
}

const RARITIES = new Set([
  "common",
  "uncommon",
  "rare",
  "epic",
  "legendary",
  "mythic",
  "unique",
  "exotic",
]);

function safeRarity(r: string | null | undefined): string {
  return r && RARITIES.has(r) ? r : "common";
}

export type SellNft = {
  name: string;
  category: string;
  rarity: string;
  network: "ethereum" | "polygon";
};

export function toSellNft(asset: OwnedAsset): SellNft {
  return {
    name: asset.name ?? "Untitled",
    category: asset.category ?? "wearable",
    rarity: safeRarity(asset.rarity),
    network: asset.network === "ETHEREUM" ? "ethereum" : "polygon",
  };
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
type SellSharedKeys = Extract<
  keyof RsProfileWearable,
  keyof z.input<typeof OwnedAssetSchema>
>;
export type _DriftSellOwnedAsset = Assert<
  AssignableTo<
    Pick<RsProfileWearable, SellSharedKeys>,
    Pick<z.input<typeof OwnedAssetSchema>, SellSharedKeys>
  >
>;
