import { ChainId } from "@dcl/schemas";
import fs from "fs";

type CollectionsMetadata = {
  height: number;
  addresses: string[];
};

export function loadCollections(): CollectionsMetadata {
  const chainId = process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET;
  const file = fs.readFileSync(
    `./assets/collections_${
      +chainId === ChainId.MATIC_MAINNET ? "mainnet" : "amoy"
    }.json`,
    "utf-8"
  );
  return JSON.parse(file) as CollectionsMetadata;
}
