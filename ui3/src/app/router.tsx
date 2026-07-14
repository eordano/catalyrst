import type { QueryClient } from "@tanstack/react-query";
import type { ComponentType } from "react";
import { lazy, Suspense } from "react";
import { createHashRouter, Navigate } from "react-router";
import type { RouteObject } from "react-router";

import AppLayout from "./AppLayout";

type PanelModule = {
  default: ComponentType;
  prefetch?: (queryClient: QueryClient) => void;
};

const panelModules = import.meta.glob<PanelModule>("./panels/*.route.{jsx,tsx}");

function idFromPath(p: string): string {
  const m = p.match(/\/([^/]+)\.route\.[jt]sx$/);
  const id = m?.[1];
  return id ? id.toLowerCase() : p;
}

export const panelLoaders: Record<string, () => Promise<PanelModule>> =
  Object.fromEntries(
    Object.entries(panelModules).map(([p, loader]) => [idFromPath(p), loader] as const),
  );

export function prefetchPanel(queryClient: QueryClient, id: string) {
  const loader = panelLoaders[id];
  if (!loader) return;
  loader()
    .then((mod) => {
      if (typeof mod.prefetch === "function") {
        try {
          mod.prefetch(queryClient);
        } catch {
        }
      }
    })
    .catch(() => {
    });
}

const childRoutes: RouteObject[] = Object.entries(panelLoaders).map(([id, loader]) => {
  const Panel = lazy(loader);
  return { path: id, element: <Panel /> };
});

childRoutes.push({ index: true, element: null });

childRoutes.push({ path: "*", element: <Navigate to="/" replace /> });

export const router = createHashRouter([
  {
    path: "/",
    element: <AppLayout prefetchPanel={prefetchPanel} />,
    children: childRoutes,
  },
]);

export { Suspense };
