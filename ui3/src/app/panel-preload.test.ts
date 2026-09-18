import { QueryClient } from "@tanstack/react-query";
import { expect, it, vi } from "vitest";
import { createPanelPreloader } from "./panel-preload";

it("warms every panel and its data while isolating failures and reusing imports for another viewer", async () => {
  const client = new QueryClient();
  const data = vi.fn();
  const places = vi.fn(async () => ({ default: () => null, prefetch: data }));
  const settings = vi.fn(async () => ({ default: () => null }));
  const broken = vi.fn(async () => { throw new Error("offline"); });
  const preload = createPanelPreloader({ places, settings, broken });
  await preload.all(client, "alice");
  expect(data).toHaveBeenCalledWith(client, "alice");
  expect(settings).toHaveBeenCalledOnce();
  await preload.all(client, "bob");
  expect(places).toHaveBeenCalledOnce();
  expect(data).toHaveBeenLastCalledWith(client, "bob");
  expect(broken).toHaveBeenCalledTimes(2);
  client.clear();
});

it("shares the module import while query freshness, invalidation and viewer identity govern data reads", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const read = vi.fn(async (address: string | null | undefined) => address);
  const loader = vi.fn(async () => ({ default: () => null, prefetch: (qc: QueryClient, address?: string | null) =>
    qc.prefetchQuery({ queryKey: ["items", address], staleTime: 30_000, queryFn: () => read(address) }),
  }));
  const preload = createPanelPreloader({ backpack: loader });
  await Promise.all([preload(client, "backpack", "alice"), preload(client, "backpack", "alice")]);
  expect(loader).toHaveBeenCalledTimes(1);
  expect(read).toHaveBeenCalledTimes(1);
  await preload(client, "backpack", "bob");
  expect(read).toHaveBeenLastCalledWith("bob");
  await client.invalidateQueries({ queryKey: ["items", "alice"] });
  await preload(client, "backpack", "alice");
  expect(read).toHaveBeenCalledTimes(3);
  client.clear();
});

it("retries failed imports and failed reads on the next intent without a 30-second lockout", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const read = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue([]);
  const loader = vi.fn().mockRejectedValueOnce(new Error("chunk unavailable")).mockResolvedValue({
    default: () => null,
    prefetch: (qc: QueryClient) => qc.prefetchQuery({ queryKey: ["places"], queryFn: read }),
  });
  const preload = createPanelPreloader({ places: loader });
  await preload(client, "places");
  await preload(client, "places");
  await preload(client, "places");
  expect(loader).toHaveBeenCalledTimes(2);
  expect(read).toHaveBeenCalledTimes(2);
  expect(client.getQueryData(["places"])).toEqual([]);
  await preload(client, "unknown");
  client.clear();
});
