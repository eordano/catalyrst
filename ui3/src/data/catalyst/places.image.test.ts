import { describe, it, expect } from "vitest";

import { localImageUrl } from "./places";
import { serviceBase } from "./client";

const HASH = "bafkreie3quzk3yvhcppse7mdupq5n63xg67iiithyxjfywjspfcaqdx4ny";

describe("localImageUrl", () => {
  it("preserves catalog content origins because historical thumbnails may not exist locally", () => {
    for (const host of ["peer", "peer-ec1", "peer-eu1", "peer-ap1", "peer-ec2"]) {
      expect(localImageUrl(`https://${host}.decentraland.org/content/contents/${HASH}`)).toBe(
        `https://${host}.decentraland.org/content/contents/${HASH}`,
      );
    }
  });

  it("maps map renders to the map service with the query intact", () => {
    const q = "?height=1024&width=1024&selected=25%2C72&center=25%2C73&size=20";
    expect(localImageUrl(`https://api.decentraland.org/v2/map.png${q}`)).toBe(
      `${serviceBase("map")}/v2/map.png${q}`,
    );
  });

  it("leaves other hosts, deeper map paths, relative or invalid URLs untouched and maps empty to undefined", () => {
    for (const other of ["https://example.com/some/image.png", "https://api.decentraland.org/v2/map.png/extra", "not a url"]) {
      expect(localImageUrl(other)).toBe(other);
    }
    expect(localImageUrl(null)).toBeUndefined();
    expect(localImageUrl("")).toBeUndefined();
    expect(localImageUrl(undefined)).toBeUndefined();
  });
});
