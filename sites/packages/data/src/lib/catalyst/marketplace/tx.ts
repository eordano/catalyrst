
import { manaToWei } from "./money";

const MARKETPLACE_V2_POLYGON =
  "0x480a0f4e360e8964e68858dd231c2922f1df45ef";

function buildBidTypedData(args: {
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

type PreparedBid = { typedData: ReturnType<typeof buildBidTypedData> };

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

