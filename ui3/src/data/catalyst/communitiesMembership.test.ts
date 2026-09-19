import { afterEach, expect, test, vi } from "vitest";
import * as client from "./client";
import { loadCommunities, loadCommunity } from "./communitiesSchema";

const community = { id: "public-community", name: "A community", description: "", ownerAddress: "0xowner", privacy: "public", membersCount: 4, isLive: false, role: "member" };
afterEach(() => vi.restoreAllMocks());

test("membership reads use signed GETs so joining survives a list reload", async () => {
  const signed = vi.spyOn(client, "sendSignedJSON").mockResolvedValue({ data: { results: [community] } });
  const publicRead = vi.spyOn(client, "getJSON").mockResolvedValue({ data: { results: [{ ...community, role: null }] } });
  expect((await loadCommunities({ limit: 20 }, { authenticated: true }))[0]?.role).toBe("member");
  expect(signed).toHaveBeenCalledWith("/v1/communities", expect.objectContaining({ method: "GET", service: "communities", query: { limit: 20 } }));
  expect(publicRead).not.toHaveBeenCalled();
  expect((await loadCommunities())[0]?.role).toBeNull();
});

test("community details authenticate the role lookup too", async () => {
  const signed = vi.spyOn(client, "sendSignedJSON").mockResolvedValue({ data: community });
  vi.spyOn(client, "getJSON").mockResolvedValue({ data: { results: [] } });
  expect((await loadCommunity(community.id, { authenticated: true }))?.community.role).toBe("member");
  expect(signed).toHaveBeenCalledWith(`/v1/communities/${community.id}`, expect.objectContaining({ method: "GET" }));
});
