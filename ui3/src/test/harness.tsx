import { StrictMode, lazy, useContext } from "react";
import type { ReactNode } from "react";
import { act, render } from "@testing-library/react";
import type { RenderResult } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { UserEvent } from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createMemoryRouter, Navigate, RouterProvider } from "react-router";
import type { RouteObject } from "react-router";
import { onTestFinished, vi } from "vitest";

import AppLayout from "../app/AppLayout";
import BootGate from "../app/BootGate";
import { panelLoaders, prefetchPanel } from "../app/router";
import { FakeBridge } from "./fakeBridge";
import { SIDEBAR_DESIGN_STORAGE } from "../data/sidebarDesignFlag";
import { WorldEntryContext } from "../app/WorldEntry";
import * as sidebarPolicy from "../data/sidebarDesignFlag";

export { makeFriend } from "./fakeBridge";

function useLegacySidebar(): void {
  const mocks = [
    vi.spyOn(sidebarPolicy, "sidebarDesignEnabled").mockReturnValue(localStorage.getItem(SIDEBAR_DESIGN_STORAGE) === "1"),
  ];
  onTestFinished(() => mocks.forEach(mock => mock.mockRestore()));
}

function installBridge(bridge: FakeBridge): void {
  const prev = window.dclBridge;
  window.dclBridge = bridge;
  onTestFinished(() => {
    window.dclBridge = prev;
  });
}

function disableNetwork(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(() =>
      Promise.reject(
        new Error("[hud-test] network disabled \u{2014} components render honest-empty"),
      ),
    ),
  );
  onTestFinished(() => {
    vi.unstubAllGlobals();
  });
}

function makeQueryClient(): QueryClient {
  const qc = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        refetchOnWindowFocus: false,
        staleTime: Infinity,
      },
    },
  });
  onTestFinished(() => {
    qc.clear();
  });
  return qc;
}

function buildRoutes(): RouteObject[] {
  const children: RouteObject[] = Object.entries(panelLoaders).map(
    ([id, loader]) => {
      const Panel = lazy(loader);
      return { path: id, element: <Panel /> };
    },
  );
  children.push({ index: true, element: null });
  children.push({ path: "*", element: <Navigate to="/" replace /> });
  return [
    {
      path: "/",
      element: <AppLayout prefetchPanel={prefetchPanel} />,
      children,
    },
  ];
}

type RenderHudOptions = {
  bridge?: FakeBridge;
  route?: string;
  minimapShown?: boolean;
  legacyHud?: boolean;
  initialView?: "lobby" | "world";
};

const MINIMAP_HIDDEN_KEY = "dcl.minimap.userHidden";

function seedMinimapPreference(shown: boolean): void {
  if (shown) localStorage.setItem(MINIMAP_HIDDEN_KEY, "0");
  else localStorage.removeItem(MINIMAP_HIDDEN_KEY);
  onTestFinished(() => {
    localStorage.removeItem(MINIMAP_HIDDEN_KEY);
  });
}

type HudHarness = RenderResult & {
  bridge: FakeBridge;
  router: ReturnType<typeof createMemoryRouter>;
  user: UserEvent;
  path: () => string;
  navigate: (to: string) => Promise<void>;
};

export function renderHud(options: RenderHudOptions = {}): HudHarness {
  if (options.legacyHud !== false) useLegacySidebar();
  const bridge = options.bridge ?? new FakeBridge();
  bridge.wrapDispatch = (fn) => act(fn);
  installBridge(bridge);
  disableNetwork();
  seedMinimapPreference(options.minimapShown ?? true);
  const queryClient = makeQueryClient();
  const router = createMemoryRouter(buildRoutes(), {
    initialEntries: [options.route ?? "/"],
  });
  const view = render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <WorldEntryContext.Provider value={options.initialView === "lobby" ? null : { pending: false, enter: () => {} }}>
          <RouterProvider router={router} />
        </WorldEntryContext.Provider>
      </QueryClientProvider>
    </StrictMode>,
  );
  const user = userEvent.setup();
  return {
    ...view,
    bridge,
    router,
    user,
    path: () => router.state.location.pathname,
    navigate: (to: string) => act(() => router.navigate(to)),
  };
}

export type BootHarness = RenderResult & {
  bridge: FakeBridge;
  queryClient: QueryClient;
};

export function renderBoot(
  options: { bridge?: FakeBridge; children?: ReactNode } = {},
): BootHarness {
  const bridge = options.bridge ?? new FakeBridge();
  bridge.wrapDispatch = (fn) => act(fn);
  installBridge(bridge);
  disableNetwork();
  const queryClient = makeQueryClient();
  const view = render(
    <QueryClientProvider client={queryClient}>
      <BootGate>{options.children ?? <BootContent />}</BootGate>
    </QueryClientProvider>,
  );
  return { ...view, bridge, queryClient };
}

export function BootContent() {
  const entry = useContext(WorldEntryContext);
  return entry?.pending ? <div aria-label="Lobby controls">
    <button onClick={() => entry.enter({ kind: "parcel", x: 0, y: 0 })}>Enter Genesis Plaza</button>
    <button onClick={() => entry.enter(null)}>Enter linked destination</button>
  </div> : <div data-testid="world-content">World controls</div>;
}
