import type { ComponentType } from "react";
import { createRoutesStub, useLocation } from "react-router";
import { HttpResponse, delay, http } from "msw";
import type { HttpHandler } from "msw";

export { HttpResponse, delay, http };
export type { HttpHandler };

const ROUTE_ID = "story-route";

type StubComponent = ComponentType<Record<string, unknown>>;
const asStub = (c: ComponentType<never>) => c as StubComponent;

function EmptyFallback() {
  return null;
}

function NavTarget() {
  const location = useLocation();
  return (
    <div style={{ padding: 32, font: "14px/1.5 monospace" }}>
      <p>
        story stub &#x2014; navigated to <strong>{location.pathname + location.search}</strong>
      </p>
    </div>
  );
}

export type RouteStoryOptions = {
  Component: ComponentType<never>;
  path?: string;
  url?: string;
  loaderData?: unknown;
  loader?: (args: { request: Request; params: Record<string, string | undefined> }) => unknown;
  action?: (args: { request: Request }) => unknown;
  extraRoutes?: { path: string; Component?: ComponentType<never> }[];
};

export function routeStory(opts: RouteStoryOptions) {
  const path = opts.path ?? "/";
  const url = opts.url ?? path;
  if (url.includes(":")) {
    throw new Error(`routeStory: pass a concrete url for parameterized path "${path}"`);
  }
  const loader =
    opts.loader ?? (opts.loaderData !== undefined ? () => opts.loaderData : undefined);
  const Stub = createRoutesStub([
    {
      id: ROUTE_ID,
      path,
      Component: asStub(opts.Component),
      loader,
      action: opts.action,
      HydrateFallback: EmptyFallback,
    },
    ...(opts.extraRoutes ?? []).map((r, i) => ({
      id: `story-extra-${i}`,
      path: r.path,
      Component: r.Component ? asStub(r.Component) : NavTarget,
    })),
    { id: "story-catchall", path: "*", Component: NavTarget },
  ]);
  const hydrationData =
    opts.loaderData !== undefined ? { loaderData: { [ROUTE_ID]: opts.loaderData } } : undefined;
  return function RouteStory() {
    return <Stub initialEntries={[url]} hydrationData={hydrationData} />;
  };
}

const FIXTURES = import.meta.glob("@data/fixtures/*.json", {
  eager: true,
  import: "default",
}) as Record<string, unknown>;

export function fixture<T>(name: string): T {
  const hit = FIXTURES[`@data/fixtures/${name}.json`];
  if (hit === undefined) {
    const have = Object.keys(FIXTURES)
      .map((k) => k.slice("@data/fixtures/".length, -".json".length))
      .join(", ");
    throw new Error(`No fixture @data/fixtures/${name}.json \u{2014} have: ${have}`);
  }
  return hit as T;
}

export const CATALYST_BASE = "https://catalyst.example.com";

type JsonHandlerInit = { status?: number; delayMs?: number };

function jsonResolver(body: unknown, init?: JsonHandlerInit) {
  return async () => {
    if (init?.delayMs) await delay(init.delayMs);
    return HttpResponse.json(body, { status: init?.status ?? 200 });
  };
}

export function catalystGet(path: string, body: unknown, init?: JsonHandlerInit): HttpHandler {
  return http.get(`${CATALYST_BASE}${path}`, jsonResolver(body, init));
}

export function catalystPost(path: string, body: unknown, init?: JsonHandlerInit): HttpHandler {
  return http.post(`${CATALYST_BASE}${path}`, jsonResolver(body, init));
}

export function jsonGet(url: string, body: unknown, init?: JsonHandlerInit): HttpHandler {
  return http.get(url, jsonResolver(body, init));
}
