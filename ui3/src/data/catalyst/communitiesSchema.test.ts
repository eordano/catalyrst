import { describe, it, expect } from "vitest";

import {
  CommunitySchema,
  isOwnAssetHost,
  normalizeCommunity,
  normalizeCommunityThumbnail,
} from "./communitiesSchema";
import { serviceBase } from "./client";

const ID = "e99471aa-31c4-4952-abf6-99905445f43b";

const base = {
  id: ID,
  name: "Winterfest Crew",
  description: "",
  ownerAddress: "0x0000000000000000000000000000000000000001",
  privacy: "public",
  membersCount: 12,
  isLive: false,
};

function parseCommunity(wire: unknown) {
  return normalizeCommunity(CommunitySchema.parse(wire));
}

describe("normalizeCommunity thumbnailUrl", () => {
  it("nulls the N/A sentinel and missing values, repoints cdn.decentraland.org thumbnails, and leaves other hosts and paths alone", () => {
    expect(parseCommunity({ ...base, thumbnailUrl: "N/A" }).thumbnailUrl).toBeNull();
    expect(parseCommunity(base).thumbnailUrl).toBeNull();
    expect(
      parseCommunity({
        ...base,
        thumbnailUrl: `https://cdn.decentraland.org/social/communities/${ID}/raw-thumbnail.png`,
      }).thumbnailUrl,
    ).toBe(`${serviceBase("communitiesCdn")}/social/communities/${ID}/raw-thumbnail.png`);
    const assets = `https://assets-cdn.decentraland.org/social/communities/${ID}/raw-thumbnail.png`;
    expect(parseCommunity({ ...base, thumbnailUrl: assets }).thumbnailUrl).toBe(assets);
    const own = `${serviceBase("communitiesCdn")}/social/communities/${ID}/raw-thumbnail.png`;
    expect(parseCommunity({ ...base, thumbnailUrl: own }).thumbnailUrl).toBe(own);
    const other = "https://cdn.decentraland.org/some/other/asset.png";
    expect(parseCommunity({ ...base, thumbnailUrl: other }).thumbnailUrl).toBe(other);
  });

  it("routes only the deployment's own assets-cdn subdomain through the same-origin CDN base, and only for thumbnails", () => {
    const site = window.location.hostname;
    const alias = `https://assets-cdn.${site}/social/communities/${ID}/raw-thumbnail.png`;
    expect(parseCommunity({ ...base, thumbnailUrl: alias }).thumbnailUrl).toBe(
      `${serviceBase("communitiesCdn")}/social/communities/${ID}/raw-thumbnail.png`,
    );
    const banner = `https://assets-cdn.${site}/social/communities/${ID}/banner.png`;
    expect(normalizeCommunityThumbnail(banner)).toBe(banner);
    expect(isOwnAssetHost("assets-cdn.catalyst.example.com", "catalyst.example.com")).toBe(true);
    expect(isOwnAssetHost("catalyst.example.com", "catalyst.example.com")).toBe(false);
    expect(isOwnAssetHost("assets-cdn.decentraland.org", "catalyst.example.com")).toBe(false);
    expect(isOwnAssetHost("evil-catalyst.example.com", "catalyst.example.com")).toBe(false);
    expect(isOwnAssetHost("assets-cdn.catalyst.example.com", "")).toBe(false);
    expect(
      normalizeCommunityThumbnail(
        `https://assets-cdn.catalyst.example.com/social/communities/${ID}/raw-thumbnail.png`,
        "catalyst.example.com",
      ),
    ).toBe(`${serviceBase("communitiesCdn")}/social/communities/${ID}/raw-thumbnail.png`);
  });
});
