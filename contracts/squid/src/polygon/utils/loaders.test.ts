import assert from "node:assert";
import { afterEach, describe, it } from "node:test";

import { ChainId } from "@dcl/schemas";
import { loadCollections } from "./loaders";

const saved = process.env.POLYGON_CHAIN_ID;

afterEach(() => {
  if (saved === undefined) delete process.env.POLYGON_CHAIN_ID;
  else process.env.POLYGON_CHAIN_ID = saved;
});

const ADDRESS = /^0x[0-9a-f]{40}$/;

const MAINNET_SNAPSHOT_HEIGHT = 92560518;
const MAINNET_SNAPSHOT_COLLECTIONS = 5902;
const AMOY_SNAPSHOT_HEIGHT = 43349033;
const AMOY_SNAPSHOT_COLLECTIONS = 224;

function assertWellFormed(addresses: string[]) {
  assert.deepStrictEqual(
    addresses.filter((a) => !ADDRESS.test(a)),
    []
  );
  assert.strictEqual(new Set(addresses).size, addresses.length);
}

describe("loadCollections", () => {
  it("reads the mainnet snapshot when POLYGON_CHAIN_ID is unset", () => {
    delete process.env.POLYGON_CHAIN_ID;
    const collections = loadCollections();
    assert.ok(collections.height >= MAINNET_SNAPSHOT_HEIGHT);
    assert.ok(collections.addresses.length >= MAINNET_SNAPSHOT_COLLECTIONS);
    assertWellFormed(collections.addresses);
  });

  it("reads the same mainnet snapshot for an explicit chain id", () => {
    process.env.POLYGON_CHAIN_ID = ChainId.MATIC_MAINNET.toString();
    const collections = loadCollections();
    assert.ok(collections.height >= MAINNET_SNAPSHOT_HEIGHT);
    assert.ok(collections.addresses.length >= MAINNET_SNAPSHOT_COLLECTIONS);
  });

  it("reads the amoy snapshot on the testnet chain id", () => {
    process.env.POLYGON_CHAIN_ID = ChainId.MATIC_AMOY.toString();
    const collections = loadCollections();
    assert.ok(collections.height >= AMOY_SNAPSHOT_HEIGHT);
    assert.ok(collections.addresses.length >= AMOY_SNAPSHOT_COLLECTIONS);
    assertWellFormed(collections.addresses);
  });

  it("keeps the two snapshots distinct", () => {
    process.env.POLYGON_CHAIN_ID = ChainId.MATIC_MAINNET.toString();
    const mainnet = loadCollections();
    process.env.POLYGON_CHAIN_ID = ChainId.MATIC_AMOY.toString();
    const amoy = loadCollections();
    assert.notStrictEqual(mainnet.height, amoy.height);
    assert.deepStrictEqual(
      mainnet.addresses.filter((a) => amoy.addresses.includes(a)),
      []
    );
  });
});
