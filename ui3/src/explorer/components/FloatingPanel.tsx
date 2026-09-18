import type { ReactNode, RefObject } from "react";
import { useLayoutEffect, useRef } from "react";
import "./floatingpanel.css";

export type FloatingPanelId = "notifications" | "voice" | "portables" | "skybox" | "friends" | "chat" | "gallery";

export const FLOATING_PANEL_TITLES: Record<FloatingPanelId, string> = {
  notifications: "Notifications",
  voice: "Nearby voice",
  portables: "Portable experiences",
  skybox: "Time of day",
  friends: "Friends",
  chat: "Chat",
  gallery: "Gallery",
};

export const PANEL_MARGIN = 16;
const MIN_ANCHORED_HEIGHT = 240;

type AnchorRect = { top: number; bottom: number; height: number };

export function anchorBeside(
  button: AnchorRect | null,
  panelHeight: number,
  viewportHeight: number,
  margin = PANEL_MARGIN,
): { top: number; maxHeight: number } {
  const limit = viewportHeight - margin;
  let top = margin;
  if (button && button.height > 0) {
    const fitsBelow = button.top + panelHeight <= limit;
    top =
      fitsBelow || limit - button.top >= MIN_ANCHORED_HEIGHT
        ? button.top
        : button.bottom - panelHeight;
  }
  top = Math.min(Math.max(margin, top), Math.max(margin, limit));
  return { top, maxHeight: Math.max(0, limit - top) };
}

function sidebarButtonFor(id: string): Element | null {
  if (typeof document === "undefined") return null;
  return document.querySelector(`[data-sb-panel="${id}"]`);
}

export function useSidebarAnchor(id: string, ref: RefObject<HTMLElement | null>, enabled = true): void {
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return undefined;
    const measure = () => {
      if (!el.isConnected) return;
      const btn = sidebarButtonFor(id);
      const body = el.querySelector<HTMLElement>(".fp__body");
      const h = Math.max(el.scrollHeight, el.getBoundingClientRect().height)
        + (body ? Math.max(0, body.scrollHeight - body.clientHeight) : 0);
      let { top, maxHeight } = anchorBeside(
        btn ? btn.getBoundingClientRect() : null,
        h,
        window.innerHeight,
      );
      const location = id === "skybox" ? document.querySelector<HTMLElement>('#sidebar-location, [data-drawer="location"]') : null;
      if (location) {
        top = location.getBoundingClientRect().bottom + 8;
        maxHeight = Math.max(0, window.innerHeight - top - PANEL_MARGIN);
      }
      el.style.top = `${top}px`;
      el.style.maxHeight = `${maxHeight}px`;
    };
    const onScroll = (event: Event) => {
      if (event.target instanceof Node && el.contains(event.target)) return;
      measure();
    };
    measure();
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", onScroll, true);
    const ro =
      typeof ResizeObserver === "function"
        ? new ResizeObserver(() => window.requestAnimationFrame(measure))
        : null;
    ro?.observe(el);
    const button = sidebarButtonFor(id);
    if (button) ro?.observe(button);
    if (id === "skybox") {
      const location = document.querySelector('#sidebar-location, [data-drawer="location"]');
      if (location) ro?.observe(location);
    }
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", onScroll, true);
      ro?.disconnect();
    };
  }, [id, ref, enabled]);
}

type FloatingPanelProps = {
  id: FloatingPanelId;
  onClose: () => void;
  title?: string;
  closeLabel?: string;
  flush?: boolean;
  actions?: ReactNode;
  anchorEnabled?: boolean;
  children: ReactNode;
};

export default function FloatingPanel({
  id,
  onClose,
  title,
  closeLabel = "Close",
  flush = false,
  actions,
  anchorEnabled = true,
  children,
}: FloatingPanelProps) {
  const ref = useRef<HTMLElement>(null);
  useSidebarAnchor(id, ref, anchorEnabled);
  const heading = title ?? FLOATING_PANEL_TITLES[id];
  return (
    <section
      ref={ref}
      className={"fp fp--" + id + (flush ? " fp--flush" : "")}
      data-hud-panel={id}
      aria-label={heading}
    >
      <header className="fp__head">
        <h2 className="fp__title">{heading}</h2>
        {actions}
        <button type="button" className="fp__close" aria-label={closeLabel} onClick={onClose}>
          <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true"
            fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M5 5l10 10M15 5L5 15" />
          </svg>
        </button>
      </header>
      <div className="fp__body">{children}</div>
    </section>
  );
}
