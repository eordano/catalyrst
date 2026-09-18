import { render, screen, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { afterEach, expect, it } from "vitest";
import { publicFreshnessKey, usePublicResult } from "./usePublicResult";

const key = ["places", {}];
const clients: QueryClient[] = [];
afterEach(() => { cleanup(); clients.splice(0).forEach(client => client.clear()); });

function mount(age: number, fail: boolean, sourceAge?: number) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  clients.push(client);
  if (sourceAge !== undefined) client.setQueryData(publicFreshnessKey(key), { updatedAt: Date.now() - sourceAge, refreshing: false, refreshFailed: false });
  function View() {
    const result = useQuery({ queryKey: key, initialData: ["Saved place"], initialDataUpdatedAt: Date.now() - age,
      queryFn: () => fail ? Promise.reject(new Error("offline")) : new Promise<string[]>(() => {}),
    });
    const query = usePublicResult(result, key);
    return <p>{query.isPending ? "Loading" : query.data?.join(",")}{query.refreshFailed && " \u2014 refresh failed"}</p>;
  }
  render(<QueryClientProvider client={client}><View /></QueryClientProvider>);
}

it("keeps usable content visible and reports a failed background refresh", async () => {
  mount(60_000, true);
  expect(await screen.findByText("Saved place \u2014 refresh failed")).toBeVisible();
});

it("does not expose cached results older than five minutes while a fresh read is pending", () => {
  mount(301_000, false);
  expect(screen.getByText("Loading")).toBeVisible();
  expect(screen.queryByText("Saved place")).toBeNull();
});

it("uses the server source age instead of treating a just-delivered stale response as new", () => {
  mount(0, false, 301_000);
  expect(screen.getByText("Loading")).toBeVisible();
});
