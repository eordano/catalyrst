import { isAddress, pad, toHex } from "viem";

export type Hex = `0x${string}`;

export const TRADE_ASSET_TYPE = {
  erc20: 1,
  usdPeggedMana: 2,
  erc721: 3,
  collectionItem: 4,
} as const;

export type TradeNetwork = "ETHEREUM" | "MATIC";

export type OffchainMarketplace = {
  chainId: number;
  address: Hex;
  name: string;
  version: string;
  network: TradeNetwork;
  networkLabel: string;
  manaAddress: Hex;
};

const OFFCHAIN_MARKETPLACES: Record<number, OffchainMarketplace> = {
  1: {
    chainId: 1,
    address: "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7",
    name: "DecentralandMarketplaceEthereum",
    version: "1.0.0",
    network: "ETHEREUM",
    networkLabel: "Ethereum",
    manaAddress: "0x0f5d2fb29fb7d3cfee444a200298f468908cc942",
  },
  11155111: {
    chainId: 11155111,
    address: "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7",
    name: "DecentralandMarketplaceEthereum",
    version: "1.0.0",
    network: "ETHEREUM",
    networkLabel: "Ethereum Sepolia",
    manaAddress: "0xfa04d2e2ba9aec166c93dfeeba7427b2303befa9",
  },
  137: {
    chainId: 137,
    address: "0xa40b1d129b8906888720686f3a01921ddf37716f",
    name: "DecentralandMarketplacePolygon",
    version: "1.0.0",
    network: "MATIC",
    networkLabel: "Polygon",
    manaAddress: "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
  },
  80002: {
    chainId: 80002,
    address: "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7",
    name: "DecentralandMarketplacePolygon",
    version: "1.0.0",
    network: "MATIC",
    networkLabel: "Polygon Amoy",
    manaAddress: "0x7ad72b9f944ea9793cf4055d88f81138cc2c63a0",
  },
};

export function offchainMarketplaceFor(chainId: number): OffchainMarketplace {
  const contract = OFFCHAIN_MARKETPLACES[chainId];
  if (!contract) {
    throw new Error(
      `listing unavailable: no off-chain marketplace is deployed on chain ${chainId}`,
    );
  }
  return contract;
}

export function supportedListingChains(): number[] {
  return Object.keys(OFFCHAIN_MARKETPLACES).map(Number);
}

const ESTATE_REGISTRIES: Record<number, string> = {
  1: "0x959e104e1a4db6317fa58f8295f586e1a978c297",
  11155111: "0x369a7fbe718c870c79f99fb423882e8dd8b20486",
};

export function isEstateRegistry(chainId: number, contractAddress: string): boolean {
  return ESTATE_REGISTRIES[chainId] === contractAddress.trim().toLowerCase();
}

export function toAddress(value: string, what: string): Hex {
  const trimmed = value.trim();
  if (!isAddress(trimmed, { strict: false })) {
    throw new Error(`listing unavailable: ${what} is not an address`);
  }
  return trimmed.toLowerCase() as Hex;
}

export type TradeExternalCheck = {
  contractAddress: string;
  selector: string;
  value: string;
  required: boolean;
};

export type TradeChecks = {
  uses: number;
  expiration: number;
  effective: number;
  salt: Hex;
  contractSignatureIndex: number;
  signerSignatureIndex: number;
  allowedRoot: Hex;
  externalChecks: TradeExternalCheck[];
};

export type SentAsset = {
  assetType: typeof TRADE_ASSET_TYPE.erc721;
  contractAddress: Hex;
  tokenId: string;
  extra: string;
};

export type ReceivedAsset = {
  assetType: typeof TRADE_ASSET_TYPE.erc20;
  contractAddress: Hex;
  amount: string;
  extra: string;
  beneficiary: Hex;
};

export type ListingTrade = {
  signer: Hex;
  network: TradeNetwork;
  chainId: number;
  type: "public_nft_order";
  checks: TradeChecks;
  sent: SentAsset[];
  received: ReceivedAsset[];
};

export type SignedListingTrade = ListingTrade & { signature: string };

export type TypedDataField = { name: string; type: string };

export const OFFCHAIN_MARKETPLACE_TYPES: Record<string, TypedDataField[]> = {
  Trade: [
    { name: "checks", type: "Checks" },
    { name: "sent", type: "AssetWithoutBeneficiary[]" },
    { name: "received", type: "Asset[]" },
  ],
  Asset: [
    { name: "assetType", type: "uint256" },
    { name: "contractAddress", type: "address" },
    { name: "value", type: "uint256" },
    { name: "extra", type: "bytes" },
    { name: "beneficiary", type: "address" },
  ],
  AssetWithoutBeneficiary: [
    { name: "assetType", type: "uint256" },
    { name: "contractAddress", type: "address" },
    { name: "value", type: "uint256" },
    { name: "extra", type: "bytes" },
  ],
  Checks: [
    { name: "uses", type: "uint256" },
    { name: "expiration", type: "uint256" },
    { name: "effective", type: "uint256" },
    { name: "salt", type: "bytes32" },
    { name: "contractSignatureIndex", type: "uint256" },
    { name: "signerSignatureIndex", type: "uint256" },
    { name: "allowedRoot", type: "bytes32" },
    { name: "externalChecks", type: "ExternalCheck[]" },
  ],
  ExternalCheck: [
    { name: "contractAddress", type: "address" },
    { name: "selector", type: "bytes4" },
    { name: "value", type: "bytes" },
    { name: "required", type: "bool" },
  ],
};

const EIP712_DOMAIN_TYPE: TypedDataField[] = [
  { name: "name", type: "string" },
  { name: "version", type: "string" },
  { name: "verifyingContract", type: "address" },
  { name: "salt", type: "bytes32" },
];

function toSeconds(ms: number): string {
  return Math.floor(ms / 1000).toString();
}

function orEmptyBytes(extra: string): string {
  return extra ? extra : "0x";
}

export function tradeValues(trade: ListingTrade) {
  return {
    checks: {
      uses: trade.checks.uses,
      expiration: toSeconds(trade.checks.expiration),
      effective: toSeconds(trade.checks.effective),
      salt: pad(trade.checks.salt, { size: 32 }),
      contractSignatureIndex: trade.checks.contractSignatureIndex,
      signerSignatureIndex: trade.checks.signerSignatureIndex,
      allowedRoot: pad(trade.checks.allowedRoot, { size: 32 }),
      externalChecks: trade.checks.externalChecks.map((check) => ({
        contractAddress: check.contractAddress,
        selector: check.selector,
        value: orEmptyBytes(check.value),
        required: check.required,
      })),
    },
    sent: trade.sent.map((asset) => ({
      assetType: asset.assetType,
      contractAddress: asset.contractAddress,
      value: asset.tokenId,
      extra: orEmptyBytes(asset.extra),
    })),
    received: trade.received.map((asset) => ({
      assetType: asset.assetType,
      contractAddress: asset.contractAddress,
      value: asset.amount,
      extra: orEmptyBytes(asset.extra),
      beneficiary: asset.beneficiary,
    })),
  };
}

export type TradeTypedData = {
  domain: { name: string; version: string; verifyingContract: Hex; salt: Hex };
  types: Record<string, TypedDataField[]>;
  primaryType: "Trade";
  message: ReturnType<typeof tradeValues>;
};

export function tradeTypedData(trade: ListingTrade): TradeTypedData {
  const market = offchainMarketplaceFor(trade.chainId);
  return {
    domain: {
      name: market.name,
      version: market.version,
      verifyingContract: market.address,
      salt: pad(toHex(trade.chainId), { size: 32 }),
    },
    types: { EIP712Domain: EIP712_DOMAIN_TYPE, ...OFFCHAIN_MARKETPLACE_TYPES },
    primaryType: "Trade",
    message: tradeValues(trade),
  };
}

export function randomSalt(): Hex {
  const source = globalThis.crypto;
  if (!source?.getRandomValues) {
    throw new Error(
      "listing unavailable: no secure random source to salt the trade signature",
    );
  }
  return toHex(source.getRandomValues(new Uint8Array(32)));
}

export type BuildListingTradeInput = {
  signer: string;
  chainId: number;
  contractAddress: string;
  tokenId: string;
  priceWei: string;
  expiresAt: number;
  effectiveAt: number;
  contractSignatureIndex: number;
  signerSignatureIndex: number;
  fingerprint?: string;
  salt?: Hex;
};

export type ListingTerms = {
  tokenId: string;
  priceWei: string;
  expiresAt: number;
  effectiveAt: number;
};

export function assertListable(terms: ListingTerms): bigint {
  let price: bigint;
  try {
    price = BigInt(terms.priceWei);
  } catch {
    throw new Error("listing unavailable: the price is not a whole wei amount");
  }
  if (price <= 0n) {
    throw new Error("listing unavailable: the price must be above zero");
  }
  if (!/^\d+$/.test(terms.tokenId)) {
    throw new Error("listing unavailable: the token id is not a number");
  }
  if (terms.expiresAt <= terms.effectiveAt) {
    throw new Error("listing unavailable: the expiration date is in the past");
  }
  return price;
}

export function buildListingTrade(input: BuildListingTradeInput): ListingTrade {
  const market = offchainMarketplaceFor(input.chainId);
  const signer = toAddress(input.signer, "the seller wallet");
  const price = assertListable(input);

  if (isEstateRegistry(input.chainId, input.contractAddress) && !input.fingerprint) {
    throw new Error(
      "listing unavailable: an estate listing needs the estate fingerprint, otherwise the signature stays valid after the estate is emptied",
    );
  }

  return {
    signer,
    network: market.network,
    chainId: market.chainId,
    type: "public_nft_order",
    checks: {
      uses: 1,
      expiration: input.expiresAt,
      effective: input.effectiveAt,
      salt: input.salt ?? randomSalt(),
      contractSignatureIndex: input.contractSignatureIndex,
      signerSignatureIndex: input.signerSignatureIndex,
      allowedRoot: "0x",
      externalChecks: [],
    },
    sent: [
      {
        assetType: TRADE_ASSET_TYPE.erc721,
        contractAddress: toAddress(input.contractAddress, "the item contract"),
        tokenId: input.tokenId,
        extra: input.fingerprint ?? "",
      },
    ],
    received: [
      {
        assetType: TRADE_ASSET_TYPE.erc20,
        contractAddress: market.manaAddress,
        amount: price.toString(),
        extra: "",
        beneficiary: signer,
      },
    ],
  };
}
