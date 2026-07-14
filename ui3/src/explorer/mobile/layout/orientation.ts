import { useSyncExternalStore } from "react";

export type Orientation = "portrait" | "landscape";

export const PORTRAIT_MEDIA_QUERY = "(orientation: portrait)";

export const SERVER_ORIENTATION: Orientation = "landscape";

export const DESIGN_BASE_LANDSCAPE = { width: 1600, height: 720 } as const;
export const DESIGN_BASE_PORTRAIT = { width: 720, height: 1600 } as const;

export function designBase(orientation: Orientation): { width: number; height: number } {
  return orientation === "portrait" ? DESIGN_BASE_PORTRAIT : DESIGN_BASE_LANDSCAPE;
}

let cachedQuery: MediaQueryList | null | undefined;

function portraitQuery(): MediaQueryList | null {
  if (cachedQuery !== undefined) return cachedQuery;
  cachedQuery =
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia(PORTRAIT_MEDIA_QUERY)
      : null;
  return cachedQuery;
}

export function readViewportOrientation(): Orientation {
  if (typeof window === "undefined") return SERVER_ORIENTATION;
  const query = portraitQuery();
  if (query) return query.matches ? "portrait" : "landscape";
  const view = window.visualViewport;
  const width = view ? view.width : window.innerWidth;
  const height = view ? view.height : window.innerHeight;
  return height >= width ? "portrait" : "landscape";
}

function subscribeViewportOrientation(onStoreChange: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const query = portraitQuery();
  const view = window.visualViewport;
  if (query && typeof query.addEventListener === "function") {
    query.addEventListener("change", onStoreChange);
  }
  window.addEventListener("resize", onStoreChange);
  window.addEventListener("orientationchange", onStoreChange);
  if (view) view.addEventListener("resize", onStoreChange);
  return () => {
    if (query && typeof query.removeEventListener === "function") {
      query.removeEventListener("change", onStoreChange);
    }
    window.removeEventListener("resize", onStoreChange);
    window.removeEventListener("orientationchange", onStoreChange);
    if (view) view.removeEventListener("resize", onStoreChange);
  };
}

export function useViewportOrientation(): Orientation {
  return useSyncExternalStore(
    subscribeViewportOrientation,
    readViewportOrientation,
    () => SERVER_ORIENTATION,
  );
}
