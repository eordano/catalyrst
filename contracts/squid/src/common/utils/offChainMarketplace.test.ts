import assert from "node:assert";
import { describe, it } from "node:test";

// getAddresses picks the address book from the chain id, so this has to be set before the module under
// test resolves it.
process.env.POLYGON_CHAIN_ID = "80002";

import { Network } from "@dcl/schemas";

import { TradedEventArgs } from "../../abi/DecentralandMarketplaceEthereum";
import { getTradeEventData, TradeAssetType } from "./offChainMarketplace";
import { MANA } from "../../polygon/addresses/amoy";

const SELLER = "0x2a4f9a28ba76413ef182351d864cc2916e462c3b";
const BUYER = "0x747c6f502272129bf1ba872a1903045b837ee86c";
const TREASURY = "0x3eedeceafa4797d36819c1d9f8e3b0071285ad69";
const COLLECTION = "0x03b1940d80394614a5ba60abbf73fa749068bdad";
/** The CreditsManager on Amoy -- the msg.sender for every credits-funded purchase. */
const CONTRACT = "0x8052a560e6e6ac86eeb7e711a4497f639b322fb3";

type Asset = {
  assetType: number;
  contractAddress: string;
  value: bigint;
  beneficiary: string;
  extra: string;
};

const asset = (over: Partial<Asset> = {}): Asset => ({
  assetType: TradeAssetType.ERC721,
  contractAddress: COLLECTION,
  value: 1n,
  beneficiary: BUYER,
  extra: "0x",
  ...over,
});

const manaAsset = (beneficiary: string, assetType = TradeAssetType.ERC20) =>
  asset({ assetType, contractAddress: MANA, value: 4079992178284085849n, beneficiary });

/** A Traded event, shaped like the ABI decoder produces it. Only the fields the mapping reads. */
const tradedEvent = (opts: {
  signer: string;
  caller: string;
  sent: Asset[];
  received: Asset[];
}) =>
  ({
    _caller: opts.caller,
    _signature: "0x" + "0".repeat(64),
    _trade: { signer: opts.signer, sent: opts.sent, received: opts.received },
  } as unknown as TradedEventArgs);

const indexable = (
  event: TradedEventArgs,
  network: Parameters<typeof getTradeEventData>[1]
) => {
  const data = getTradeEventData(event, network);
  assert.ok(data, "expected an indexable trade");
  return data;
};

/**
 * Who a sale is attributed to.
 *
 * The payment leg's `beneficiary` means "who gets paid", and for years that was also "who sold" --
 * nobody redirected a payment. The Shop does: it signs listings that pay the platform treasury, which
 * credits the seller off-chain. Reading the seller off that beneficiary silently reassigned every such
 * sale to the treasury, which is what these tests pin against.
 */
describe("getTradeEventData \u{2014} sale attribution", () => {
  describe("an order (a listing, signed by its seller)", () => {
    it("should attribute the sale to the signer, not to whoever was paid", () => {
      const event = tradedEvent({
        signer: SELLER,
        caller: BUYER,
        sent: [asset({ beneficiary: BUYER })],
        received: [manaAsset(TREASURY, TradeAssetType.USD_PEGGED_MANA)],
      });

      const data = indexable(event, Network.MATIC);

      assert.strictEqual(data.seller, SELLER);
      assert.strictEqual(data.buyer, BUYER);
    });

    it("should be immune to a contract standing in for msg.sender", () => {
      const event = tradedEvent({
        signer: SELLER,
        caller: CONTRACT,
        sent: [asset({ beneficiary: BUYER })],
        received: [manaAsset(TREASURY, TradeAssetType.USD_PEGGED_MANA)],
      });

      const data = indexable(event, Network.MATIC);

      assert.strictEqual(data.seller, SELLER);
      assert.strictEqual(data.buyer, BUYER);
    });

    it("should be unchanged when the seller is paid directly", () => {
      const event = tradedEvent({
        signer: SELLER,
        caller: BUYER,
        sent: [asset({ beneficiary: BUYER })],
        received: [manaAsset(SELLER)],
      });

      const data = indexable(event, Network.MATIC);

      assert.strictEqual(data.seller, SELLER);
      assert.strictEqual(data.buyer, BUYER);
    });

    it("should still read the asset side for the buyer and the price", () => {
      const event = tradedEvent({
        signer: SELLER,
        caller: BUYER,
        sent: [asset({ beneficiary: BUYER, value: 42n })],
        received: [manaAsset(TREASURY, TradeAssetType.USD_PEGGED_MANA)],
      });

      const data = indexable(event, Network.MATIC);

      assert.strictEqual(data.collectionAddress, COLLECTION);
      assert.strictEqual(data.tokenId, 42n);
      assert.strictEqual(data.price, 4079992178284085849n);
    });
  });

  describe("a bid (signed by its buyer \u{2014} the seller is not in the signed trade)", () => {
    it("should read the seller from the payment beneficiary, not from msg.sender", () => {
      const event = tradedEvent({
        signer: BUYER,
        caller: CONTRACT,
        sent: [manaAsset(SELLER)],
        received: [asset({ beneficiary: BUYER })],
      });

      const data = indexable(event, Network.MATIC);

      assert.strictEqual(data.seller, SELLER);
      assert.strictEqual(data.buyer, BUYER);
    });

    it("should read the seller from the payment beneficiary when they called directly", () => {
      const event = tradedEvent({
        signer: BUYER,
        caller: SELLER,
        sent: [manaAsset(SELLER)],
        received: [asset({ beneficiary: BUYER })],
      });

      const data = indexable(event, Network.MATIC);

      assert.strictEqual(data.seller, SELLER);
      assert.strictEqual(data.buyer, BUYER);
    });
  });
});

/**
 * A trade with an EMPTY leg.
 *
 * A giveaway -- the signer hands over an asset and takes no payment -- is accepted by the contract and
 * emits `Traded` with `received: []`. Two such trades landed in Polygon block 91576312 and crash-looped
 * the polygon processor: it read `received[0].assetType` on `undefined`, which is fatal inside the batch
 * transaction, so it stopped indexing everything behind that block.
 */
describe("getTradeEventData \u{2014} a trade with no payment leg", () => {
  it("should not throw when `received` is empty", () => {
    const event = tradedEvent({
      signer: SELLER,
      caller: CONTRACT,
      sent: [asset({ beneficiary: BUYER })],
      received: [],
    });

    assert.doesNotThrow(() => getTradeEventData(event, Network.MATIC));
  });

  it("should not throw when `sent` is empty", () => {
    const event = tradedEvent({
      signer: SELLER,
      caller: CONTRACT,
      sent: [],
      received: [manaAsset(SELLER)],
    });

    assert.doesNotThrow(() => getTradeEventData(event, Network.MATIC));
  });

  it("should report a giveaway as not indexable rather than inventing a bid", () => {
    // The `else` branch used to catch both "it is a bid" and "we could not tell". A giveaway has no
    // price and no seller to read, so treating it as a bid would write both as fiction.
    const event = tradedEvent({
      signer: SELLER,
      caller: CONTRACT,
      sent: [asset({ beneficiary: BUYER })],
      received: [],
    });

    assert.strictEqual(getTradeEventData(event, Network.MATIC), undefined);
  });

  it("should still classify a normal order, so the guard costs nothing", () => {
    const event = tradedEvent({
      signer: SELLER,
      caller: CONTRACT,
      sent: [asset({ beneficiary: BUYER })],
      received: [manaAsset(TREASURY)],
    });

    const data = indexable(event, Network.MATIC);

    assert.strictEqual(data.seller, SELLER);
    assert.strictEqual(data.buyer, BUYER);
  });
});
