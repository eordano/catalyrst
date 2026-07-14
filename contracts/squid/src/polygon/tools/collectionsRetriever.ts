import "dotenv/config";

import { ChainId, Network } from "@dcl/schemas";
import { run } from "@subsquid/batch-processor";
import { createLogger } from "@subsquid/logger";
import { DataSourceBuilder } from "@subsquid/evm-stream";
import { Database, LocalDest } from "@subsquid/file-store";
import { publicPortalDataset } from "../../common/utils/portal";
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

const logger = createLogger("sqd:collections-retriever");

const chainId =
  process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET.toString();
process.env.POLYGON_CHAIN_ID = chainId;

const addresses = getAddresses(Network.MATIC);

const isMainnet = +chainId === ChainId.MATIC_MAINNET;

const fileName = `collections_${isMainnet ? "mainnet" : "amoy"}.json`;

const PORTAL_URL = publicPortalDataset(
  `polygon-${isMainnet ? "mainnet" : "amoy-testnet"}`
);

const dataSource = new DataSourceBuilder()
  .setPortal(PORTAL_URL)
  .setFields({
    block: { timestamp: true },
    log: { address: true, topics: true, data: true },
  })
  .addLog({
    where: {
      address: [addresses.CollectionFactory],
      topic0: [CollectionFactoryABI.events.ProxyCreated.topic],
    },
    range: { from: fromsV1[chainId] },
  })
  .addLog({
    where: {
      address: [addresses.CollectionFactoryV3],
      topic0: [CollectionFactoryV3ABI.events.ProxyCreated.topic],
    },
    range: { from: fromsV3[chainId] },
  })
  .build();

let collections: string[] = [];

type Metadata = {
  height: number;
  hash: string;
  addresses: string[];
};

let isInit = false;
let isReady = false;

let startHeight: number | undefined;
let startCount = 0;
let lastHeight = 0;

const SUSPICIOUS_EMPTY_SCAN_BLOCKS = 1_000_000;

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
          startHeight = height;
          startCount = addresses.length;
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
      await dest.writeFile(fileName, JSON.stringify(metadata, null, 2) + "\n");
    },
  },
});

run(dataSource, db, async (ctx) => {
  ctx.store.setForceFlush(true);
  if (isReady) {
    const scanned = lastHeight - (startHeight ?? lastHeight);
    const found = collections.length - startCount;
    if (found === 0 && scanned > SUSPICIOUS_EMPTY_SCAN_BLOCKS) {
      logger.fatal(
        `Scanned ${scanned} blocks up to ${lastHeight} and found NO collections. ` +
          `Filtering on CollectionFactory=${addresses.CollectionFactory} ` +
          `CollectionFactoryV3=${addresses.CollectionFactoryV3} (POLYGON_CHAIN_ID=${chainId}). ` +
          `${fileName} now claims that height with an unchanged address list -- DISCARD it, do not commit.`
      );
      process.exit(1);
    }
    logger.info(
      `done: +${found} collections over ${scanned} blocks, head ${lastHeight}`
    );
    process.exit();
  }
  if (ctx.isHead) isReady = true;
  if (ctx.blocks.length > 0) {
    lastHeight = ctx.blocks[ctx.blocks.length - 1].header.height;
  }

  for (let c of ctx.blocks) {
    for (let i of c.logs) {
      if (
        [addresses.CollectionFactory, addresses.CollectionFactoryV3]
          .map((c) => c.toLowerCase())
          .includes(i.address)
      ) {
        const { _address } = CollectionFactoryABI.events.ProxyCreated.decode(i);
        collections.push(_address.toLowerCase());
        logger.info(`collections: ${collections.length}`);
      }
    }
  }
});
