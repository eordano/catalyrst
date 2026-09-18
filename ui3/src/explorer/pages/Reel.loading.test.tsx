import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";
import Reel from "./Reel";
import { signedFetch } from "../../data/catalyst/client";

const viewer = vi.hoisted(() => ({ address: "alice" as string | null }));
vi.mock("../../overlay/bridge", async original => {
  const actual = await original<typeof import("../../overlay/bridge")>();
  return { ...actual, useBridgeState: (select: (state: unknown) => unknown) => select({
    ...actual.FALLBACK_STATE, identity: { ...actual.FALLBACK_STATE.identity, address: viewer.address },
  }) };
});
vi.mock("../../data/catalyst/client", async original => ({
  ...await original<typeof import("../../data/catalyst/client")>(), signedFetch: vi.fn(),
}));
afterEach(() => { cleanup(); vi.resetAllMocks(); viewer.address = "alice"; });

it("does not flash empty content or reuse another account's photos while loading", async () => {
  type Response = Awaited<ReturnType<typeof signedFetch>>;
  let resolve!: (value: Response) => void;
  vi.mocked(signedFetch).mockImplementation(() => new Promise(done => { resolve = done; }));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(<QueryClientProvider client={client}><MemoryRouter><Reel embedded /></MemoryRouter></QueryClientProvider>);
  expect(screen.getByRole("status")).toHaveTextContent("Loading your reel");
  expect(screen.queryByText("There are no photos yet")).toBeNull();
  await act(async () => resolve({ status: 200, body: JSON.stringify({ images: [{ id: "a", url: "https://images.example/a.png" }] }) } as Response));
  expect(screen.getByAltText("Reel photo")).toBeVisible();
  viewer.address = "bob";
  view.rerender(<QueryClientProvider client={client}><MemoryRouter><Reel embedded /></MemoryRouter></QueryClientProvider>);
  expect(screen.getByRole("status")).toHaveTextContent("Loading your reel");
  expect(screen.queryByAltText("Reel photo")).toBeNull();
  await act(async () => resolve({ status: 200, body: JSON.stringify({ images: [] }) } as Response));
  expect(screen.getByText("There are no photos yet")).toBeVisible();
  viewer.address = null;
  view.rerender(<QueryClientProvider client={client}><MemoryRouter><Reel embedded /></MemoryRouter></QueryClientProvider>);
  expect(screen.getByText("Sign in to see your photos")).toBeVisible();
});
