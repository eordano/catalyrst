import { encodeFunctionData, toHex } from "viem";

import { manaToWei } from "./money";

export const MARKETPLACE_ABI = [
  {
    type: "function",
    name: "executeOrder",
    stateMutability: "nonpayable",
    inputs: [
      { name: "nftAddress", type: "address" },
      { name: "assetId", type: "uint256" },
      { name: "price", type: "uint256" },
    ],
    outputs: [],
  },
] as const;

export type ExecuteOrderParams = {
  marketplaceAddress: string;
  contractAddress: string;
  tokenId: string;
  priceWei: string;
  chainId: number;
};

export function buildExecuteOrderCalldata(p: ExecuteOrderParams): `0x${string}` {
  return encodeFunctionData({
    abi: MARKETPLACE_ABI,
    functionName: "executeOrder",
    args: [p.contractAddress as `0x${string}`, BigInt(p.tokenId), BigInt(p.priceWei)],
  });
}

export function buildMetaTxTypedData(args: {
  marketplaceAddress: string;
  chainId: number;
  from: string;
  functionSignature: `0x${string}`;
  nonce?: bigint;
}) {
  return {
    domain: {
      name: "Decentraland Marketplace",
      version: "2",
      verifyingContract: args.marketplaceAddress,
      salt: toHex(BigInt(args.chainId), { size: 32 }),
    },
    types: {
      EIP712Domain: [
        { name: "name", type: "string" },
        { name: "version", type: "string" },
        { name: "verifyingContract", type: "address" },
        { name: "salt", type: "bytes32" },
      ],
      MetaTransaction: [
        { name: "nonce", type: "uint256" },
        { name: "from", type: "address" },
        { name: "functionSignature", type: "bytes" },
      ],
    },
    primaryType: "MetaTransaction" as const,
    message: {
      nonce: (args.nonce ?? 0n).toString(),
      from: args.from,
      functionSignature: args.functionSignature,
    },
  };
}

export type PreparedMetaTx = {
  functionSignature: `0x${string}`;
  typedData: ReturnType<typeof buildMetaTxTypedData>;
};

export function prepareBuyMetaTx(
  p: ExecuteOrderParams & { from: string; nonce?: bigint },
): PreparedMetaTx {
  const functionSignature = buildExecuteOrderCalldata(p);
  const typedData = buildMetaTxTypedData({
    marketplaceAddress: p.marketplaceAddress,
    chainId: p.chainId,
    from: p.from,
    functionSignature,
    nonce: p.nonce,
  });
  return { functionSignature, typedData };
}

export const MARKETPLACE_V2_POLYGON =
  "0x480a0f4e360e8964e68858dd231c2922f1df45ef";

export function buildBidTypedData(args: {
  chainId: number;
  bidder: string;
  tokenAddress: string;
  tokenId: string;
  priceWei: string;
  expiresAt: number;
  verifyingContract?: string;
}) {
  const verifyingContract = (
    args.verifyingContract ?? MARKETPLACE_V2_POLYGON
  ).toLowerCase();
  const bidder = args.bidder.toLowerCase();
  const tokenAddress = args.tokenAddress.toLowerCase();
  return {
    domain: {
      name: "Decentraland Bid",
      version: "2",
      chainId: args.chainId,
      verifyingContract,
    },
    types: {
      EIP712Domain: [
        { name: "name", type: "string" },
        { name: "version", type: "string" },
        { name: "chainId", type: "uint256" },
        { name: "verifyingContract", type: "address" },
      ],
      Bid: [
        { name: "bidder", type: "address" },
        { name: "tokenAddress", type: "address" },
        { name: "tokenId", type: "uint256" },
        { name: "price", type: "uint256" },
        { name: "expiresAt", type: "uint256" },
      ],
    },
    primaryType: "Bid" as const,
    message: {
      bidder,
      tokenAddress,
      tokenId: args.tokenId,
      price: args.priceWei,
      expiresAt: String(args.expiresAt),
    },
  };
}

export type PreparedBid = { typedData: ReturnType<typeof buildBidTypedData> };

export function prepareBid(args: {
  chainId: number;
  bidder: string;
  tokenAddress: string;
  tokenId: string;
  priceMana: number;
  expiration?: string;
}): PreparedBid {
  const expiresAt = args.expiration
    ? Math.floor(Date.parse(args.expiration) / 1000)
    : Math.floor(Date.now() / 1000) + 30 * 86_400;
  return {
    typedData: buildBidTypedData({
      chainId: args.chainId,
      bidder: args.bidder,
      tokenAddress: args.tokenAddress,
      tokenId: args.tokenId,
      priceWei: manaToWei(args.priceMana),
      expiresAt,
    }),
  };
}

export function buildNameClaimTypedData(args: {
  chainId: number;
  beneficiary: string;
  name: string;
  priceWei: string;
}) {
  return {
    domain: {
      name: "Decentraland Name Registrar",
      version: "1",
      chainId: args.chainId,
    },
    types: {
      EIP712Domain: [
        { name: "name", type: "string" },
        { name: "version", type: "string" },
        { name: "chainId", type: "uint256" },
      ],
      RegisterName: [
        { name: "beneficiary", type: "address" },
        { name: "name", type: "string" },
        { name: "price", type: "uint256" },
      ],
    },
    primaryType: "RegisterName" as const,
    message: {
      beneficiary: args.beneficiary.toLowerCase(),
      name: args.name,
      price: args.priceWei,
    },
  };
}

export type PreparedNameClaim = {
  typedData: ReturnType<typeof buildNameClaimTypedData>;
};

export function prepareNameClaim(args: {
  chainId: number;
  beneficiary: string;
  name: string;
  priceMana?: number;
}): PreparedNameClaim {
  return {
    typedData: buildNameClaimTypedData({
      chainId: args.chainId,
      beneficiary: args.beneficiary,
      name: args.name,
      priceWei: manaToWei(args.priceMana ?? 100),
    }),
  };
}

