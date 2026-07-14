import { describe, expect, test, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createMemoryRouter, RouterProvider } from "react-router";
import { onTestFinished } from "vitest";

import EventsPanel from "./Events.route";
import { FakeBridge } from "../../test/fakeBridge";
import { parseEvent } from "../../data/catalyst/events";
import { qk } from "../../data/queryKeys";

const recordDefaults = {
  all_day: false,
  position: [0, 0],
  coordinates: [0, 0],
  live: false,
  highlighted: false,
  trending: false,
  recurrent: false,
  total_attendees: 0,
  world: false,
};

const worldEvent = parseEvent({
  ...recordDefaults,
  id: "w1",
  name: "World Bash",
  world: true,
  server: "kickoff.dcl.eth",
  x: 0,
  y: 0,
  live: true,
});

const parcelEvent = parseEvent({
  ...recordDefaults,
  id: "p1",
  name: "Plaza Party",
  x: 10,
  y: -20,
  position: [10, -20],
  coordinates: [10, -20],
  live: true,
  highlighted: true,
});

function renderEvents() {
  const bridge = new FakeBridge();
  const prev = window.dclBridge;
  window.dclBridge = bridge;
  vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("network disabled"))));
  onTestFinished(() => {
    window.dclBridge = prev;
    vi.unstubAllGlobals();
  });
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  qc.setQueryData(qk.events({ limit: 100 }), {
    data: [worldEvent, parcelEvent],
    total: 2,
  });
  qc.setQueryData(qk.eventCategories(), []);
  onTestFinished(() => qc.clear());
  const router = createMemoryRouter([{ path: "/", element: <EventsPanel /> }]);
  render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { bridge, user: userEvent.setup() };
}

describe("events jump in", () => {
  test("world event confirms then changes realm instead of teleporting", async () => {
    const { bridge, user } = renderEvents();
    await user.click(screen.getByRole("button", { name: "Jump in to World Bash" }));

    bridge.expectNotSent("Teleport");
    bridge.expectNotSent("ChangeRealm");
    const modal = screen.getByRole("dialog", { name: "Visit world" });
    expect(within(modal).getByText("kickoff.dcl.eth")).toBeInTheDocument();

    await user.click(within(modal).getByRole("button", { name: "CONTINUE" }));
    bridge.expectSent("ChangeRealm", { realm: "kickoff.dcl.eth" });
    bridge.expectNotSent("Teleport");
    expect(screen.getByRole("status")).toHaveTextContent("Teleporting to World Bash");
  });

  test("cancelling the world confirm sends nothing", async () => {
    const { bridge, user } = renderEvents();
    await user.click(screen.getByRole("button", { name: "Jump in to World Bash" }));

    const modal = screen.getByRole("dialog", { name: "Visit world" });
    await user.click(within(modal).getByRole("button", { name: "CANCEL" }));
    expect(screen.queryByRole("dialog", { name: "Visit world" })).toBeNull();
    bridge.expectNotSent("ChangeRealm");
    bridge.expectNotSent("Teleport");
  });

  test("parcel event teleports on the current realm", async () => {
    const { bridge, user } = renderEvents();
    const featured = screen.getByRole("complementary", { name: "Featured event" });
    await user.click(within(featured).getByRole("button", { name: "jump in" }));

    bridge.expectSent("Teleport", { x: 10 * 16 + 8, z: -20 * 16 + 8 });
    bridge.expectNotSent("ChangeRealm");
    expect(screen.queryByRole("dialog", { name: "Visit world" })).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("Teleporting to Plaza Party");
  });
});
