import { describe, it, expect } from "vitest";

import { CommunitySchema, normalizeCommunity } from "./communitiesSchema";
import { serviceBase } from "./client";

const ID = "e99471aa-31c4-4952-abf6-99905445f43b";

// The fields catalyrst-social-service always serializes; the schema now
// requires them, so a fixture that omits them is not a community at all.
const base = {
  id: ID,
  name: "Winterfest Crew",
  description: "",
  ownerAddress: "0x0000000000000000000000000000000000000001",
  privacy: "public",
  membersCount: 12,
  isLive: false,
};

// What a reader hands the UI: the shape check, then the normalization. Asserted
// through both because the rewrite has to survive the perf build, where only the
// second half still runs.
function parseCommunity(wire: unknown) {
  return normalizeCommunity(CommunitySchema.parse(wire));
}

describe("normalizeCommunity thumbnailUrl", () => {
  it("maps the service's literal N/A sentinel to null", () => {
    const c = parseCommunity({ ...base, thumbnailUrl: "N/A" });
    expect(c.thumbnailUrl).toBeNull();
  });

  it("maps missing thumbnails to null (prod omits the field unsigned)", () => {
    const c = parseCommunity(base);
    expect(c.thumbnailUrl).toBeNull();
  });

  it("repoints cdn.decentraland.org thumbnails at the communities CDN base", () => {
    const c = parseCommunity({
      ...base,
      thumbnailUrl: `https://cdn.decentraland.org/social/communities/${ID}/raw-thumbnail.png`,
    });
    expect(c.thumbnailUrl).toBe(
      `${serviceBase("communitiesCdn")}/social/communities/${ID}/raw-thumbnail.png`,
    );
  });

  it("leaves other hosts untouched (prod assets CDN, the CDN base itself)", () => {
    const assets = `https://assets-cdn.decentraland.org/social/communities/${ID}/raw-thumbnail.png`;
    expect(parseCommunity({ ...base, thumbnailUrl: assets }).thumbnailUrl).toBe(assets);

    const own = `${serviceBase("communitiesCdn")}/social/communities/${ID}/raw-thumbnail.png`;
    expect(parseCommunity({ ...base, thumbnailUrl: own }).thumbnailUrl).toBe(own);
  });

  it("does not rewrite cdn.decentraland.org outside community thumbnail paths", () => {
    const other = "https://cdn.decentraland.org/some/other/asset.png";
    expect(parseCommunity({ ...base, thumbnailUrl: other }).thumbnailUrl).toBe(other);
  });
});
