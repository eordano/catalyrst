import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import NotificationsPanel from "./Notifications.route";
import { fetchLiveNotifications } from "../../data/catalyst/notifications";
import { qk } from "../../data/queryKeys";

const viewer = vi.hoisted(() => ({ address: "alice" }));
vi.mock("../../overlay/bridge", async original => ({
  ...await original<typeof import("../../overlay/bridge")>(),
  useBridgeState: (select: (state: unknown) => unknown) => select({ identity: viewer }),
}));
vi.mock("../../overlay/minimapVisibility", () => ({ useHideMinimapWhileMounted: () => {} }));
vi.mock("../../data/catalyst/notifications", async original => ({
  ...await original<typeof import("../../data/catalyst/notifications")>(),
  fetchLiveNotifications: vi.fn(),
}));
const clients: QueryClient[] = [];
afterEach(() => { cleanup(); clients.splice(0).forEach(client => client.clear()); vi.resetAllMocks(); viewer.address = "alice"; });
function client() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  clients.push(qc);
  return qc;
}

it("shows pending, failure, retry and authoritative empty states without demo notifications", async () => {
  let reject!: (error: Error) => void;
  vi.mocked(fetchLiveNotifications).mockImplementationOnce(() => new Promise((_, fail) => { reject = fail; }));
  render(<QueryClientProvider client={client()}><NotificationsPanel /></QueryClientProvider>);
  expect(screen.getByRole("status")).toHaveTextContent("Loading notifications");
  expect(screen.queryByText("Friend Request Received")).toBeNull();
  expect(screen.queryByText("You're all caught up")).toBeNull();
  await act(async () => reject(new Error("offline")));
  await screen.findByRole("alert");
  expect(screen.queryByText("You're all caught up")).toBeNull();
  vi.mocked(fetchLiveNotifications).mockResolvedValueOnce([]);
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await screen.findByText("You're all caught up");
  expect(screen.queryByRole("alert")).toBeNull();
});

it("keeps cached notifications after a refresh error and clears them on account change", async () => {
  const qc = client();
  qc.setQueryData(qk.notifications("alice"), [{ id: "notice", type: "system", timestamp: Date.now(), read: true, metadata: { title: "Saved notification" } }]);
  vi.mocked(fetchLiveNotifications).mockRejectedValue(new Error("offline"));
  const view = render(<QueryClientProvider client={qc}><NotificationsPanel /></QueryClientProvider>);
  await screen.findByRole("alert");
  expect(document.querySelectorAll(".nf__card")).toHaveLength(1);
  vi.mocked(fetchLiveNotifications).mockImplementation(() => new Promise(() => {}));
  viewer.address = "bob";
  view.rerender(<QueryClientProvider client={qc}><NotificationsPanel /></QueryClientProvider>);
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Loading notifications"));
  expect(document.querySelectorAll(".nf__card")).toHaveLength(0);
});
