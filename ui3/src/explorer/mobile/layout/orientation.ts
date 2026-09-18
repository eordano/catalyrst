import { useSyncExternalStore } from "react";

export type Orientation = "portrait" | "landscape";

const PORTRAIT_MEDIA_QUERY = "(orientation: portrait)";

const SERVER_ORIENTATION: Orientation = "landscape";

let cachedQuery: MediaQueryList | null | undefined;

function portraitQuery(): MediaQueryList | null {
  if (cachedQuery !== undefined) return cachedQuery;
  cachedQuery =
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia(PORTRAIT_MEDIA_QUERY)
      : null;
  return cachedQuery;
}

function readViewportOrientation(): Orientation {
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
