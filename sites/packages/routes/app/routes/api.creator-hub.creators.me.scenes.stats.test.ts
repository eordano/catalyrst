import { beforeEach, expect, it, vi } from "vitest";
import { generatePrivateKey } from "viem/accounts";
import { createIdentityFromPrivateKey } from "@data/lib/auth/identity";
import { signRequest } from "@data/lib/auth/signer";
import { loadCreatorSceneStats } from "@data/lib/catalyst/creator-hub/scene-analytics.server";
import { loader } from "./api.creator-hub.creators.me.scenes.stats";

vi.mock("@data/lib/catalyst/creator-hub/scene-analytics.server", () => ({ loadCreatorSceneStats: vi.fn() }));
beforeEach(() => vi.resetAllMocks());
const url = "http://localhost/api/creator-hub/creators/me/scenes/stats";
it("rejects anonymous requests before accessing activity", async () => {
  const response = await loader({ request: new Request(url) });
  expect(response.status).toBe(401);
  expect(loadCreatorSceneStats).not.toHaveBeenCalled();
  expect(response.headers.get("cache-control")).toBe("private, no-store");
});
it("scopes the source to the verified signer rather than a query or wallet cookie", async () => {
  const identity = await createIdentityFromPrivateKey(generatePrivateKey());
  const { headers } = await signRequest(identity, "GET", url);
  vi.mocked(loadCreatorSceneStats).mockResolvedValue({ address: identity.signer, as_of: "2026-09-18", scenes: [] });
  const response = await loader({ request: new Request(`${url}?address=0xother`, { headers: { ...headers, cookie: "dcl_wallet=0xother" } }) });
  expect(response.status).toBe(200);
  expect(loadCreatorSceneStats).toHaveBeenCalledWith(identity.signer, expect.objectContaining({ signal: expect.any(AbortSignal) }));
  vi.mocked(loadCreatorSceneStats).mockRejectedValue(new Error("Database unreachable"));
  expect((await loader({ request: new Request(url, { headers }) })).status).toBe(503);
});
