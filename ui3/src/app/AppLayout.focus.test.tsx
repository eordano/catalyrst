import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { WORLD_CANVAS_ID } from "../explorer/mobile";
import { FakeBridge } from "../test/fakeBridge";
import AppLayout from "./AppLayout";

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
  vi.unstubAllGlobals();
  await new Promise((r) => setTimeout(r, 0));
});

function mount() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  const router = createMemoryRouter(
    [{ path: "/", element: <AppLayout />, children: [{ index: true, element: null }] }],
    { initialEntries: ["/"] },
  );
  render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return userEvent.setup();
}

const settle = () => act(() => new Promise((r) => setTimeout(r, 5)));

describe("overlay pointer-up focus guard", () => {
  test("a pointer release inside an open menu keeps the menu focused; anywhere else hands focus back to the world canvas", async () => {
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
