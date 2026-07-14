import assert from "node:assert";
import { describe, it } from "node:test";

import * as MarketplaceEthereum from "../../abi/DecentralandMarketplaceEthereum";
import * as MarketplaceEthereumV3 from "../../abi/DecentralandMarketplaceEthereumV3";
import * as MarketplacePolygon from "../../polygon/abi/DecentralandMarketplacePolygon";
import * as MarketplacePolygonV3 from "../../polygon/abi/DecentralandMarketplacePolygonV3";

// Both topics are pinned so a regenerated or mis-wired ABI module breaks a test instead of matching
// no logs at all: filtering a V3 address with the V1/V2 topic fails silently.
const TRADED_TOPIC_V1_V2 =
  "0xaaecdfa7e74e704650fcb273f630f42f68974eff42bfffc1732cf30db9e4685b";
const TRADED_TOPIC_V3 =
  "0x71dc7036c75ab7570a8b79d4a452c5a4d3ac4fdf0b2cc58d518d979f0ec557ff";

describe("Traded event topics", () => {
  describe("when reading the V1/V2 marketplace modules", () => {
    it("should expose the original Traded topic on Ethereum", () => {
      assert.equal(
        MarketplaceEthereum.events.Traded.topic,
        TRADED_TOPIC_V1_V2
      );
    });

    it("should expose the original Traded topic on Polygon", () => {
      assert.equal(MarketplacePolygon.events.Traded.topic, TRADED_TOPIC_V1_V2);
    });
  });

  describe("when reading the V3 marketplace modules", () => {
    it("should expose the new Traded topic on Ethereum", () => {
      assert.equal(MarketplaceEthereumV3.events.Traded.topic, TRADED_TOPIC_V3);
    });

    it("should expose the new Traded topic on Polygon", () => {
      assert.equal(MarketplacePolygonV3.events.Traded.topic, TRADED_TOPIC_V3);
    });

    it("should declare _tradeDigest, which is what moves the topic", () => {
      assert.ok("_tradeDigest" in MarketplacePolygonV3.events.Traded.params);
    });
  });

  describe("and comparing the V3 modules against V1/V2", () => {
    it("should not share the Ethereum topic, so V3 logs need the V3 module to match", () => {
      assert.notEqual(
        MarketplaceEthereumV3.events.Traded.topic,
        MarketplaceEthereum.events.Traded.topic
      );
    });

    it("should not share the Polygon topic, so V3 logs need the V3 module to match", () => {
      assert.notEqual(
        MarketplacePolygonV3.events.Traded.topic,
        MarketplacePolygon.events.Traded.topic
      );
    });
  });
});
