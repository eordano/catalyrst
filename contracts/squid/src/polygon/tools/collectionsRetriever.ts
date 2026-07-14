import "dotenv/config";

import { ChainId, Network } from "@dcl/schemas";
import { assertNotNull } from "@subsquid/util-internal";
import { EvmBatchProcessor } from "@subsquid/evm-processor";
import { Database, LocalDest } from "@subsquid/file-store";
import { getAddresses } from "../../common/utils/addresses";
import * as CollectionFactoryABI from "../abi/CollectionFactory";
import * as CollectionFactoryV3ABI from "../abi/CollectionFactoryV3";

const fromsV1: Record<string, any> = {
  [ChainId.MATIC_MAINNET]: 15202000,
  [ChainId.MATIC_AMOY]: 14517370,
};

const fromsV3: Record<string, any> = {
  [ChainId.MATIC_MAINNET]: 28121692,
  [ChainId.MATIC_AMOY]: 5763249,
};

const addresses = getAddresses(Network.MATIC);
const chainId = process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET;

const fileName = `collections_${
  +chainId === ChainId.MATIC_MAINNET ? "mainnet" : "amoy"
}.json`;

const GATEWAY = `https://v2.archive.subsquid.io/network/polygon-${
  chainId == ChainId.MATIC_MAINNET ? "mainnet" : "amoy-testnet"
}`;

const processor = new EvmBatchProcessor()
  .setGateway({ url: GATEWAY, apiKey: process.env.SQUID_API_KEY })
  .setRpcEndpoint({
    url: assertNotNull(process.env.RPC_ENDPOINT_POLYGON),
    rateLimit: 10,
  })
  .setFields({
    log: {
      topics: true,
      data: true,
    },
  })
  .addLog({
    address: [addresses.CollectionFactory],
    topic0: [CollectionFactoryABI.events.ProxyCreated.topic],
    range: {
      from: fromsV1[chainId],
    },
  })
  .addLog({
    address: [addresses.CollectionFactoryV3],
    topic0: [CollectionFactoryV3ABI.events.ProxyCreated.topic],
    range: {
      from: fromsV3[chainId],
    },
  })
  .setFinalityConfirmation(100);

let collections: string[] = [];

type Metadata = {
  height: number;
  hash: string;
  addresses: string[];
};

let isInit = false;
let isReady = false;

let db = new Database({
  tables: {},
  dest: new LocalDest("./assets"),
  chunkSizeMb: 10,
  hooks: {
    async onStateRead(dest) {
      if (await dest.exists(fileName)) {
        let { height, hash, addresses }: Metadata = await dest
          .readFile(fileName)
          .then(JSON.parse);

        if (!isInit) {
          collections = addresses;
          isInit = true;
        }

        return { height, hash };
      } else {
        return undefined;
      }
    },
    async onStateUpdate(dest, info) {
      let metadata: Metadata = {
        ...info,
        addresses: collections,
      };
      await dest.writeFile(fileName, JSON.stringify(metadata));
    },
  },
});

processor.run(db, async (ctx) => {
  ctx.store.setForceFlush(true);
  if (isReady) process.exit();
  if (ctx.isHead) isReady = true;

  for (let c of ctx.blocks) {
    for (let i of c.logs) {
      if (
        [addresses.CollectionFactory, addresses.CollectionFactoryV3]
          .map((c) => c.toLowerCase())
          .includes(i.address)
      ) {
        const { _address } = CollectionFactoryABI.events.ProxyCreated.decode(i);
        collections.push(_address.toLowerCase());
        ctx.log.info(`collections: ${collections.length}`);
      }
    }
  }
});
