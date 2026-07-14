import { ChainId, Network } from "@dcl/schemas";
import { Multicall } from "../../abi/multicall";
import { functions as LANDRegistryFunctions } from "../../abi/LANDRegistry";
import { functions as erc721Functions } from "../../abi/ERC721";
import { Block, Context } from "../../eth/processor";
import { Coordinate } from "../../types";
import { getAddresses } from "./addresses";

const MULTICALL_CONTRACT = "0xcA11bde05977b3631167028862bE2a173976CA11"; // has the same address on different networks
const hardcodedMulticallCreationBlock = {
  id: "0014353601-7a3f0",
  number: 14353601,
  height: 14353601,
  hash: "0x7a3f054738fa78c0a9c34a0f4216e40d858a8b086a666860783e9950dbeaf647",
  parentHash:
    "0xfa3164b5d4e40d76caa767ce1ee5722b4bf88cc1566b6c45aec9b1530268377f",
  timestamp: 1646842676000,
};

export async function tokenURIMutilcall(
  ctx: Context,
  lastBlock: Block,
  tokenIds: Map<string, bigint[]> // contractAddress => tokenIds[]
): Promise<Map<string, string>> {
  const chainId = process.env.ETHEREUM_CHAIN_ID || ChainId.ETHEREUM_MAINNET;

  let multiCallBlock = lastBlock;
  if (
    chainId.toString() === ChainId.ETHEREUM_MAINNET.toString() &&
    lastBlock.height < hardcodedMulticallCreationBlock.height
  ) {
    multiCallBlock = hardcodedMulticallCreationBlock;
  }

  const multicall = new Multicall(ctx, multiCallBlock, MULTICALL_CONTRACT);

  const tokenIdsArray: [string, bigint][] = [];
  Array.from(tokenIds.entries()).forEach(([contractAddress, tokenIds]) => {
    tokenIds.forEach((tokenId) => {
      tokenIdsArray.push([contractAddress, tokenId]);
    });
  });

  const param = tokenIdsArray.map(([contractAddress, tokenId]) => [
    contractAddress,
    { _tokenId: tokenId },
  ]) as [string, { _tokenId: bigint }][];

  const results = await multicall.aggregate(
    erc721Functions.tokenURI,
    param,
    500
  );
  const tokenURIs = new Map<string, string>();
  results.forEach((res, i) =>
    tokenURIs.set(`${param[i][0]}-${param[i][1]._tokenId}`, res)
  );
  return tokenURIs;
}

export async function decodeLANDTokenIdMulticall(
  ctx: Context,
  block: Block,
  tokenIds: Set<bigint>
): Promise<Map<bigint, Coordinate>> {
  const contractAddress = getAddresses(Network.ETHEREUM).LANDRegistry;
  const multicall = new Multicall(
    ctx,
    hardcodedMulticallCreationBlock,
    MULTICALL_CONTRACT
  );

  const tokenIdsArray: [string, bigint][] = [];
  Array.from(tokenIds.values()).forEach((tokenId) => {
    tokenIdsArray.push([contractAddress, tokenId]);
  });

  const param = tokenIdsArray.map(([contractAddress, tokenId]) => {
    return [contractAddress, { value: tokenId }];
  }) as [string, { value: bigint }][];

  const results = await multicall.tryAggregate(
    LANDRegistryFunctions.decodeTokenId,
    param,
    50
  );

  const coordinates = new Map<bigint, Coordinate>();

  results.forEach((res, i) => coordinates.set(param[i][1].value, res.value!));

  return coordinates;
}
