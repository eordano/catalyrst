import { useSyncExternalStore } from "react";
import type { Orientation } from "./layout/orientation";

export type MobileOrientation = Orientation;

export type MobileOverride = "mobile" | "desktop" | null;

export type MobileEnv = {
  readonly isMobile: boolean;
  readonly coarsePointer: boolean;
  readonly anyHover: boolean;
  readonly touchPoints: number;
  readonly hasTouch: boolean;
  readonly width: number;
  readonly height: number;
  readonly shortEdge: number;
  readonly orientation: MobileOrientation;
  readonly override: MobileOverride;
  readonly uaMobileHint: boolean;
};

export const MOBILE_MAX_SHORT_EDGE = 900;
export const MOBILE_HINTED_MAX_SHORT_EDGE = 1200;
export const MOBILE_OVERRIDE_PARAM = "mobile";
export const MOBILE_OVERRIDE_STORAGE_KEY = "dcl.mobile.force";
export const SSR_VIEWPORT_WIDTH = 1600;
export const SSR_VIEWPORT_HEIGHT = 720;

const COARSE_POINTER_QUERY = "(pointer: coarse)";
const ANY_HOVER_QUERY = "(any-hover: hover)";

const SERVER_ENV: MobileEnv = Object.freeze({
  isMobile: false,
  coarsePointer: false,
  anyHover: true,
  touchPoints: 0,
  hasTouch: false,
  width: SSR_VIEWPORT_WIDTH,
  height: SSR_VIEWPORT_HEIGHT,
  shortEdge: SSR_VIEWPORT_HEIGHT,
  orientation: "landscape",
  override: null,
  uaMobileHint: false,
});

const OVERRIDE_ON = new Set(["1", "true", "mobile", "yes", "on"]);
const OVERRIDE_OFF = new Set(["0", "false", "desktop", "no", "off"]);

function parseOverride(raw: string | null): MobileOverride {
  if (raw == null) return null;
  const v = raw.trim().toLowerCase();
  if (OVERRIDE_ON.has(v)) return "mobile";
  if (OVERRIDE_OFF.has(v)) return "desktop";
  return null;
}

function queryOverride(): MobileOverride {
  try {
    const loc = window.location;
    const fromSearch = parseOverride(
      new URLSearchParams(loc.search).get(MOBILE_OVERRIDE_PARAM),
    );
    if (fromSearch) return fromSearch;
    const qi = loc.hash.indexOf("?");
    if (qi >= 0) {
      return parseOverride(
        new URLSearchParams(loc.hash.slice(qi + 1)).get(MOBILE_OVERRIDE_PARAM),
      );
    }
  } catch {
  }
  return null;
}

function storedOverride(): MobileOverride {
  try {
    return parseOverride(localStorage.getItem(MOBILE_OVERRIDE_STORAGE_KEY));
  } catch {
    return null;
  }
}

function media(query: string, fallback: boolean): boolean {
  if (typeof window.matchMedia !== "function") return fallback;
  try {
    return window.matchMedia(query).matches;
  } catch {
    return fallback;
  }
}

type NavigatorWithUaData = Navigator & { userAgentData?: { mobile?: boolean } };

function readUaMobileHint(): boolean {
  if (typeof navigator === "undefined") return false;
  return (navigator as NavigatorWithUaData).userAgentData?.mobile === true;
}

function evaluate(): MobileEnv {
  if (typeof window === "undefined") return SERVER_ENV;
  const override = queryOverride() ?? storedOverride();
  const coarsePointer = media(COARSE_POINTER_QUERY, false);
  const anyHover = media(ANY_HOVER_QUERY, true);
  const touchPoints =
    typeof navigator === "undefined" || typeof navigator.maxTouchPoints !== "number"
      ? 0
      : navigator.maxTouchPoints;
  const hasTouch = touchPoints > 0 || "ontouchstart" in window;
  const width = window.innerWidth || SSR_VIEWPORT_WIDTH;
  const height = window.innerHeight || SSR_VIEWPORT_HEIGHT;
  const shortEdge = Math.min(width, height);
  const uaMobileHint = readUaMobileHint();
  const maxShortEdge = uaMobileHint
    ? MOBILE_HINTED_MAX_SHORT_EDGE
    : MOBILE_MAX_SHORT_EDGE;
  const detected = hasTouch && coarsePointer && shortEdge <= maxShortEdge;
  return {
    isMobile: override === "mobile" ? true : override === "desktop" ? false : detected,
    coarsePointer,
    anyHover,
    touchPoints,
    hasTouch,
    width,
    height,
    shortEdge,
    orientation: height >= width ? "portrait" : "landscape",
    override,
    uaMobileHint,
  };
}

function sameEnv(a: MobileEnv, b: MobileEnv): boolean {
  return (
    a.isMobile === b.isMobile &&
    a.coarsePointer === b.coarsePointer &&
    a.anyHover === b.anyHover &&
    a.touchPoints === b.touchPoints &&
    a.hasTouch === b.hasTouch &&
    a.width === b.width &&
    a.height === b.height &&
    a.orientation === b.orientation &&
    a.override === b.override &&
    a.uaMobileHint === b.uaMobileHint
  );
}

let snapshot: MobileEnv | null = null;
const listeners = new Set<() => void>();
let detach: (() => void) | null = null;

export function getMobileEnv(): MobileEnv {
  if (snapshot == null) snapshot = evaluate();
  return snapshot;
}

export function getServerMobileEnv(): MobileEnv {
  return SERVER_ENV;
}

export function refreshMobileEnv(): void {
  const next = evaluate();
  const prev = snapshot;
  if (prev != null && sameEnv(prev, next)) return;
  snapshot = next;
  for (const l of [...listeners]) l();
}

function attach(): () => void {
  if (typeof window === "undefined") return () => {};
  const cleanups: Array<() => void> = [];
  if (typeof window.matchMedia === "function") {
    for (const q of [COARSE_POINTER_QUERY, ANY_HOVER_QUERY]) {
      try {
        const mql = window.matchMedia(q);
        mql.addEventListener("change", refreshMobileEnv);
        cleanups.push(() => mql.removeEventListener("change", refreshMobileEnv));
      } catch {
      }
    }
  }
  for (const evt of ["resize", "orientationchange", "pageshow"] as const) {
    window.addEventListener(evt, refreshMobileEnv);
    cleanups.push(() => window.removeEventListener(evt, refreshMobileEnv));
  }
  return () => {
    for (const c of cleanups) c();
  };
}

export function subscribeMobileEnv(onChange: () => void): () => void {
  listeners.add(onChange);
  if (detach == null) {
    detach = attach();
    refreshMobileEnv();
  }
  return () => {
    listeners.delete(onChange);
    if (listeners.size > 0) return;
    detach?.();
    detach = null;
    snapshot = null;
  };
}

export function setMobileOverride(next: MobileOverride): void {
  if (typeof window === "undefined") return;
  try {
    if (next == null) localStorage.removeItem(MOBILE_OVERRIDE_STORAGE_KEY);
    else localStorage.setItem(MOBILE_OVERRIDE_STORAGE_KEY, next);
  } catch {
  }
  refreshMobileEnv();
}

export function useMobileEnv(): MobileEnv {
  return useSyncExternalStore(subscribeMobileEnv, getMobileEnv, getServerMobileEnv);
}

function getIsMobile(): boolean {
  return getMobileEnv().isMobile;
}

function getServerIsMobile(): boolean {
  return SERVER_ENV.isMobile;
}

export function useIsMobile(): boolean {
  return useSyncExternalStore(subscribeMobileEnv, getIsMobile, getServerIsMobile);
}
