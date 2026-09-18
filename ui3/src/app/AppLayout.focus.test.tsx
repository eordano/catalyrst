import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { WORLD_CANVAS_ID } from "../explorer/mobile";
import { FakeBridge } from "../test/fakeBridge";
import AppLayout from "./AppLayout";
import { WorldEntryContext } from "./WorldEntry";
import * as sidebarPolicy from "../data/sidebarDesignFlag";


let canvas: HTMLCanvasElement;
let bridge: FakeBridge;

beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("offline"))));
  localStorage.setItem("dcl.minimap.userHidden", "0");
  canvas = document.createElement("canvas");
  canvas.id = WORLD_CANVAS_ID;
  canvas.tabIndex = 0;
  document.body.appendChild(canvas);
  bridge = new FakeBridge();
  bridge.wrapDispatch = (fn) => act(fn);
  window.dclBridge = bridge;
});

afterEach(async () => {
  delete window.dclBridge;
  canvas.remove();
  localStorage.removeItem("dcl.minimap.userHidden");
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await new Promise((r) => setTimeout(r, 0));
});

function mount(initial = "/", prefetchPanel?: (client: QueryClient, id: string, address?: string | null) => void, prefetchAllPanels?: (client: QueryClient, address?: string | null) => unknown) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  const router = createMemoryRouter(
    [{ path: "/", element: <AppLayout prefetchPanel={prefetchPanel} prefetchAllPanels={prefetchAllPanels} />, children: [{ index: true, element: null }, { path: "*", element: null }] }],
    { initialEntries: [initial] },
  );
  render(
    <QueryClientProvider client={qc}>
      <WorldEntryContext.Provider value={{ pending: false, enter: () => {} }}><RouterProvider router={router} /></WorldEntryContext.Provider>
    </QueryClientProvider>,
  );
  return userEvent.setup();
}

const settle = () => act(() => new Promise((r) => setTimeout(r, 5)));

test("warms every panel only after startup readiness and once per viewer", async () => {
  const preload = vi.fn();
  mount("/places", undefined, preload);
  bridge.pushIdentity({ address: "alice" });
  bridge.pushLoading({ ready: false, avatarLoaded: false });
  await settle();
  expect(preload).not.toHaveBeenCalled();
  bridge.pushLoading({ ready: true, avatarLoaded: true });
  await settle();
  expect(preload).toHaveBeenCalledWith(expect.any(QueryClient), "alice");
  bridge.pushLoading({ ready: false, avatarLoaded: true });
  bridge.pushLoading({ ready: true, avatarLoaded: true });
  await settle();
  expect(preload).toHaveBeenCalledTimes(1);
  bridge.pushIdentity({ address: "bob" });
  await settle();
  expect(preload).toHaveBeenLastCalledWith(expect.any(QueryClient), "bob");
});

test("panel intent starts on hover, keyboard focus and touch with the current viewer", () => {
  const preload = vi.fn();
  mount("/places", preload);
  bridge.pushIdentity({ address: "0x1111111111111111111111111111111111111111" });
  const button = screen.getByRole("button", { name: /Backpack/ });
  for (const fire of [fireEvent.mouseOver, fireEvent.focus, fireEvent.pointerDown]) {
    preload.mockClear();
    fire(button);
    expect(preload).toHaveBeenCalledWith(expect.any(QueryClient), "backpack", "0x1111111111111111111111111111111111111111");
  }
  bridge.pushIdentity({ address: "0x2222222222222222222222222222222222222222" });
  fireEvent.pointerDown(button);
  expect(preload).toHaveBeenLastCalledWith(expect.any(QueryClient), "backpack", "0x2222222222222222222222222222222222222222");
});

describe("overlay pointer-up focus guard", () => {
  test("a pointer release inside an open menu keeps the menu focused; anywhere else hands focus back to the world canvas", async () => {
    vi.spyOn(sidebarPolicy, "sidebarDesignEnabled").mockReturnValue(false);
    const user = mount();
    bridge.pushScene({ title: "CBD Plaza", coords: "-143,102" });
    await user.click(screen.getByRole("button", { name: "Scene options" }));
    const menu = await screen.findByRole("menu");
    const item = menu.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')!;
    item.focus();
    expect(document.activeElement).toBe(item);

    fireEvent.pointerUp(item);
    await settle();
    expect(document.activeElement).toBe(item);

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).not.toBe(canvas);
    fireEvent.pointerUp(screen.getByRole("navigation", { name: "Main menu" }));
    await settle();
    expect(document.activeElement).toBe(canvas);
  });
});

describe("world popup routes", () => {
  test.each([['friends', 'Friends'], ['skybox', 'Time of day'], ['smartwearables', 'Portable experiences']])('%s opens over the world and closes with Escape', async (route, title) => {
    const user = mount('/' + route);
    expect(await screen.findByRole('region', { name: title })).toBeTruthy();
    expect(screen.queryByRole('dialog', { name: 'Explore' })).toBeNull();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('region', { name: title })).toBeNull();
    expect(screen.queryByRole('dialog', { name: 'Explore' })).toBeNull();
  });
});

test("Gallery retains Explore navigation and closes back to the world", async () => {
  const user = mount("/gallery");
  expect(await screen.findByRole("navigation", { name: "Explore sections" })).toBeTruthy();
  expect(screen.getByRole("button", { name: /GALLERY/i })).toHaveAttribute("aria-current", "page");
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("navigation", { name: "Explore sections" })).toBeNull();
});
