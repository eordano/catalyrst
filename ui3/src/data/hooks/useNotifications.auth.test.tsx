import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useNotifications } from "./useNotifications";
import { fetchLiveNotifications } from "../catalyst/notifications";

const identity = vi.hoisted(() => ({ address: "", isGuest: true }));
vi.mock("../../overlay/bridge", () => ({ useBridgeState: (select: (state: unknown) => unknown) => select({ identity }) }));
vi.mock("../catalyst/notifications", () => ({ fetchLiveNotifications: vi.fn(async () => []), markNotificationsRead: vi.fn(), unreadCount: () => 0 }));
afterEach(() => { cleanup(); vi.clearAllMocks(); identity.address = ""; identity.isGuest = true; });

it("waits for a wallet identity before fetching private notifications", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const { result, rerender, unmount } = renderHook(useNotifications, {
    wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
  });
  expect(fetchLiveNotifications).not.toHaveBeenCalled();
  expect(result.current.isLoading).toBe(false);
  identity.address = "0x1111111111111111111111111111111111111111";
  rerender();
  expect(fetchLiveNotifications).not.toHaveBeenCalled();
  identity.isGuest = false;
  rerender();
  await waitFor(() => expect(fetchLiveNotifications).toHaveBeenCalledTimes(1));
  unmount();
  client.clear();
});
