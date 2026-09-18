import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import { coordsToPercent, toPlaceView } from "../../data/catalyst/places";
import { useBridgeState } from "../../overlay/bridge";
import { FakeBridge } from "../../test/fakeBridge";
import { qk } from "../../data/queryKeys";
import MapPanel from "./Map.route";

const SHELL = { w: 800, h: 600 };
const COORDS = "10,-20";

function Probe() {
  useBridgeState();
  return null;
}

function transformOf(el: HTMLElement) {
  const m = /translate\((-?[\d.]+)px, (-?[\d.]+)px\) scale\(([\d.]+)\)/.exec(el.style.transform);
  if (!m) throw new Error(`unexpected transform: ${el.style.transform}`);
  return { x: Number(m[1]), y: Number(m[2]), scale: Number(m[3]) };
}

function centeredPan(z: number) {
  const { left, top } = coordsToPercent(COORDS);
  const square = Math.min(SHELL.w, SHELL.h);
  return { x: -(left / 100 - 0.5) * square * z, y: -(top / 100 - 0.5) * square * z };
}

function renderMap() {
  const bridge = new FakeBridge();
  bridge.wrapDispatch = (fn) => act(fn);
  window.dclBridge = bridge;
  vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("network disabled"))));
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: SHELL.w,
    bottom: SHELL.h,
    width: SHELL.w,
    height: SHELL.h,
    toJSON: () => ({}),
  });
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  const tree = (map: boolean) => (
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <Probe />
        {map && <MapPanel />}
      </MemoryRouter>
    </QueryClientProvider>
  );
  const view = render(tree(false));
  bridge.pushScene({ coords: COORDS });
  view.rerender(tree(true));
  const tiles = view.container.querySelector<HTMLElement>(".map__tiles");
  if (!tiles) throw new Error("map tiles missing");
  return { tiles, client: qc, container: view.container, view: () => transformOf(tiles) };
}

afterEach(() => {
  delete window.dclBridge;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("expanded map zoom", () => {
  test("opening lands on the player's parcel; Recenter pans back to the parcel and keeps the zoom the user chose", () => {
    const { tiles, view } = renderMap();
    const z0 = view().scale;
    expect(view().x).toBeCloseTo(centeredPan(z0).x, 5);
    expect(view().y).toBeCloseTo(centeredPan(z0).y, 5);

    fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    const z1 = view().scale;
    expect(z1).toBeLessThan(z0);

    fireEvent.pointerDown(tiles, { clientX: 100, clientY: 100, button: 0, pointerId: 1 });
    fireEvent.pointerMove(tiles, { clientX: 160, clientY: 130, pointerId: 1 });
    fireEvent.pointerUp(tiles, { clientX: 160, clientY: 130, pointerId: 1 });
    expect(view().x).toBeCloseTo(centeredPan(z1).x + 60, 5);
    expect(view().y).toBeCloseTo(centeredPan(z1).y + 30, 5);

    fireEvent.click(screen.getByRole("button", { name: "Recenter" }));
    expect(view().scale).toBe(z1);
    expect(view().x).toBeCloseTo(centeredPan(z1).x, 5);
    expect(view().y).toBeCloseTo(centeredPan(z1).y, 5);
  });
});


test("category reads distinguish pending, failure and a confirmed empty response", async () => {
  const { client } = renderMap();
  act(() => client.setQueryData(qk.categories(), [{ name: "art", label: "Art", color: "#fff" }]));
  let reject!: (error: Error) => void;
  vi.mocked(fetch).mockImplementationOnce(() => new Promise((_, fail) => { reject = fail; }));
  fireEvent.click(await screen.findByRole("tab", { name: "ART" }));
  const sidebar = within(screen.getByRole("complementary"));
  expect(sidebar.getByRole("status")).toHaveTextContent("Loading scenes");
  expect(sidebar.queryByText("No scenes found.")).toBeNull();
  await act(async () => reject(new Error("offline")));
  expect(await sidebar.findByRole("alert")).toHaveTextContent("Couldn't load scenes");
  expect(sidebar.queryByText("No scenes found.")).toBeNull();
  vi.mocked(fetch).mockResolvedValueOnce(new Response(JSON.stringify({ ok: true, data: [], total: 0 }), { status: 200 }));
  fireEvent.click(sidebar.getByRole("button", { name: "Retry" }));
  expect(await sidebar.findByText("No scenes found.")).toBeInTheDocument();
});

test("search responds during debounce, exposes failures, and retries both feeds", async () => {
  const { container } = renderMap();
  fireEvent.change(screen.getByPlaceholderText("Search places & worlds"), { target: { value: "gallery" } });
  const results = within(container.querySelector<HTMLElement>(".map__searchresults")!);
  expect(results.getByRole("status")).toHaveTextContent("Searching places and worlds");
  expect(await results.findByRole("alert")).toHaveTextContent("Couldn't load all search results");
  expect(results.queryByText("No places or worlds found.")).toBeNull();
  vi.mocked(fetch).mockImplementation(() => Promise.resolve(new Response(JSON.stringify({ ok: true, data: [], total: 0 }), { status: 200 })));
  fireEvent.click(results.getByRole("button", { name: "Retry" }));
  expect(await results.findByText("No places or worlds found.")).toBeInTheDocument();
});


test("a category result outside the initial map opens immediately and preserves its card when details fail", async () => {
  const { client, container } = renderMap();
  const place = toPlaceView({
    id: "remote-gallery", title: "Remote Gallery", description: "A distant gallery", image: null,
    owner: null, creator_address: null, contact_name: null, base_position: "150,150", positions: ["150,150"],
    categories: ["art"], user_count: 0, user_visits: 0, favorites: 0, likes: 0, like_rate: null,
    highlighted: false, world: false, world_name: null, updated_at: null,
  });
  act(() => {
    client.setQueryData(qk.categories(), [{ name: "art", label: "Art", color: "#fff" }]);
    client.setQueryData(qk.places({ limit: 50, order_by: "most_active", order: "desc", categories: "art" }), [place]);
  });
  fireEvent.click(await screen.findByRole("tab", { name: "ART" }));
  fireEvent.click(within(screen.getByRole("complementary")).getByRole("button", { name: /Remote Gallery/ }));
  expect([...container.querySelectorAll(".map__infoname")].map(node => node.textContent)).toContain("Remote Gallery");
  fireEvent.click(screen.getByRole("button", { name: "details" }));
  expect(screen.getByRole("heading", { name: "Remote Gallery" })).toBeInTheDocument();
  expect(await screen.findByText("Couldn't refresh place details.")).toBeInTheDocument();
  expect(screen.queryByText("Place not found")).toBeNull();
});
