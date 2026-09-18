import { QueryClient } from "@tanstack/react-query";
import { expect, it, vi } from "vitest";
import { signedFetch } from "./client";
import { prefetchReel, reelQuery } from "./reel";

vi.mock("./client", () => ({ serviceBase: () => "https://reel.test", signedFetch: vi.fn(async () => ({ status: 200, body: '{"images":[],"currentImages":0,"maxImages":500}' })) }));

it("shares the first page between camera and gallery, preserving viewer isolation", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  await Promise.all([prefetchReel(client, "alice"), prefetchReel(client, "alice")]);
  await client.fetchQuery(reelQuery("alice"));
  expect(signedFetch).toHaveBeenCalledTimes(1);
  await client.fetchQuery(reelQuery("bob"));
  expect(signedFetch).toHaveBeenCalledTimes(2);
  client.clear();
});
