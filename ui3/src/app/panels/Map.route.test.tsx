import { act, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import { coordsToPercent } from "../../data/catalyst/places";
import { useBridgeState } from "../../overlay/bridge";
import { FakeBridge } from "../../test/fakeBridge";
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
  return { tiles, view: () => transformOf(tiles) };
}

afterEach(() => {
  delete window.dclBridge;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("expanded map zoom", () => {
  test("opening lands on the player's parcel at zoom 8; Recenter pans back to the parcel and keeps the zoom the user chose", () => {
    const { tiles, view } = renderMap();
    expect(view().scale).toBe(8);
    expect(view().x).toBeCloseTo(centeredPan(8).x, 5);
    expect(view().y).toBeCloseTo(centeredPan(8).y, 5);

    fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(view().scale).toBe(7.5);

    fireEvent.pointerDown(tiles, { clientX: 100, clientY: 100, button: 0, pointerId: 1 });
    fireEvent.pointerMove(tiles, { clientX: 160, clientY: 130, pointerId: 1 });
    fireEvent.pointerUp(tiles, { clientX: 160, clientY: 130, pointerId: 1 });
    expect(view().x).toBeCloseTo(centeredPan(7.5).x + 60, 5);
    expect(view().y).toBeCloseTo(centeredPan(7.5).y + 30, 5);

    fireEvent.click(screen.getByRole("button", { name: "Recenter" }));
    expect(view().scale).toBe(7.5);
    expect(view().x).toBeCloseTo(centeredPan(7.5).x, 5);
    expect(view().y).toBeCloseTo(centeredPan(7.5).y, 5);
  });
});
