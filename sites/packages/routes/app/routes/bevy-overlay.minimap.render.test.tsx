import type { ComponentType } from "react";
import { renderToString } from "react-dom/server";
import {
  createStaticHandler,
  createStaticRouter,
  StaticRouterProvider,
  type LoaderFunction,
  type RouteObject,
} from "react-router";
import { describe, expect, it } from "vitest";

import BevyOverlayMinimap from "./bevy-overlay.minimap";
import HudRoute from "./bevy-overlay.hud";

const ASSIGNMENT = {
  variant: "overlay",
  flags: { domOverlay: true },
  experimentKey: "client_hud_overlay",
};

const MINIMAP_DATA = {
  sid: "s",
  coords: "-143,102",
  place: "CBD Plaza",
  heading: null,
  assignment: ASSIGNMENT,
};

const HUD_DATA = {
  sid: "s",
  widget: null,
  assignment: ASSIGNMENT,
  realm: null,
  emotes: {
    address: "",
    catalog: [],
    loadout: [],
    slotOrder: [],
    liveEmpty: true,
    source: "empty",
    error: false,
  },
};

const routes: RouteObject[] = [
  {
    path: "/bevy-overlay/minimap",
    Component: BevyOverlayMinimap as unknown as ComponentType,
    loader: (() => MINIMAP_DATA) as LoaderFunction,
  },
  {
    path: "/bevy-overlay/hud",
    Component: HudRoute as unknown as ComponentType,
    loader: (() => HUD_DATA) as LoaderFunction,
  },
];

async function firstPaint(path: string): Promise<string> {
  const handler = createStaticHandler(routes);
  const context = await handler.query(new Request(`https://catalyst.example.com${path}`));
  if (context instanceof Response) throw new Error(`unexpected response ${context.status}`);
  const router = createStaticRouter(handler.dataRoutes, context);
  return renderToString(<StaticRouterProvider router={router} context={context} />);
}

describe("bevy-overlay story routes SSR", () => {
  it("renders the minimap and hud routes server-side without an app-level query client", async () => {
    const minimap = await firstPaint("/bevy-overlay/minimap");
    expect(minimap).toContain('aria-label="Scene options"');
    expect(minimap).toContain("-143,102");

    const hud = await firstPaint("/bevy-overlay/hud");
    expect(hud).toContain('aria-label="Connection status"');
    expect(hud).toContain('class="client-stage"');
  });
});
