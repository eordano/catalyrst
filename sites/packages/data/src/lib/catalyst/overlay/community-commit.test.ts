import { beforeEach, expect, it, vi } from "vitest";
import type { AuthIdentity } from "../../auth/types";
vi.mock("../../auth/signer", () => ({ signedFetch: vi.fn() }));
import { signedFetch } from "../../auth/signer";
import { buildCommunityJoinCommit } from "./community-commit";

beforeEach(() => vi.clearAllMocks());

it("never substitutes a simulated membership for a missing identity", async () => {
  const commit = buildCommunityJoinCommit(() => null);
  await expect(commit({ communityId: "club", action: "join" })).rejects.toThrow(/sign in/i);
  expect(signedFetch).not.toHaveBeenCalled();
});

it("uses the newly signed-in wallet and propagates a rejected write", async () => {
  let identity: AuthIdentity | null = null;
  const commit = buildCommunityJoinCommit(() => identity);
  identity = { signer: "0x123", ephemeral: { address: "0xeph", privateKey: "0x123" }, expiration: "2999-01-01", authChain: [] };
  vi.mocked(signedFetch).mockResolvedValue(new Response('{"message":"Forbidden"}', { status: 403 }));
  await expect(commit({ communityId: "club", action: "request" })).rejects.toThrow(/403/);
  expect(signedFetch).toHaveBeenCalledWith(identity, expect.stringContaining("/v1/communities/club/requests"), expect.objectContaining({ method: "POST" }));
});
