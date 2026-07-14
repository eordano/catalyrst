import { describe, it, expect } from "vitest";

import { localImageUrl } from "./places";
import { catalystBase, serviceBase } from "./client";

const HASH = "bafkreie3quzk3yvhcppse7mdupq5n63xg67iiithyxjfywjspfcaqdx4ny";

describe("localImageUrl", () => {
  it("repoints peer content URLs at the catalyst base", () => {
    for (const host of ["peer", "peer-ec1", "peer-eu1", "peer-ap1", "peer-ec2"]) {
      expect(localImageUrl(`https://${host}.decentraland.org/content/contents/${HASH}`)).toBe(
        `${catalystBase()}/content/contents/${HASH}`,
      );
    }
  });

  it("repoints api.decentraland.org map renders at the map service base, query intact", () => {
    const q = "?height=1024&width=1024&selected=25%2C72&center=25%2C73&size=20";
    expect(localImageUrl(`https://api.decentraland.org/v2/map.png${q}`)).toBe(
      `${serviceBase("map")}/v2/map.png${q}`,
    );
  });

  it("leaves non-catalyst paths untouched", () => {
    const other = "https://example.com/some/image.png";
    expect(localImageUrl(other)).toBe(other);
    const deep = "https://api.decentraland.org/v2/map.png/extra";
    expect(localImageUrl(deep)).toBe(deep);
  });

  it("passes through relative or invalid URLs unchanged", () => {
    expect(localImageUrl("not a url")).toBe("not a url");
  });

  it("maps empty and null to undefined", () => {
    expect(localImageUrl(null)).toBeUndefined();
    expect(localImageUrl("")).toBeUndefined();
    expect(localImageUrl(undefined)).toBeUndefined();
  });
});
