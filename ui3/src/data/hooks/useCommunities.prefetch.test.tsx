import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { communitiesQuery, useCommunities } from "./useCommunities";
import { loadCommunities } from "../catalyst/communitiesSchema";

const identity = vi.hoisted(() => ({ address: "alice" }));
vi.mock("../../overlay/bridge", async importOriginal => ({
  ...await importOriginal<typeof import("../../overlay/bridge")>(),
  useBridgeState: (select: (state: unknown) => unknown) => select({ identity }),
}));
vi.mock("../catalyst/communitiesSchema", () => ({ loadCommunities: vi.fn(async () => []) }));
afterEach(() => { cleanup(); vi.clearAllMocks(); identity.address = "alice"; });

it("reuses a prefetched signed list on mount and loads separately after an account switch", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  await client.prefetchQuery(communitiesQuery({}, identity.address));
  function Panel() {
    const query = useCommunities();
    return <div>{query.isSuccess ? "ready" : "loading"}</div>;
  }
  const view = render(<QueryClientProvider client={client}><Panel /></QueryClientProvider>);
  await screen.findByText("ready");
  expect(loadCommunities).toHaveBeenCalledTimes(1);
  expect(loadCommunities).toHaveBeenCalledWith({}, expect.objectContaining({ authenticated: true }));
  identity.address = "bob";
  view.rerender(<QueryClientProvider client={client}><Panel /></QueryClientProvider>);
  await screen.findByText("ready");
  expect(loadCommunities).toHaveBeenCalledTimes(2);
  client.clear();
});
