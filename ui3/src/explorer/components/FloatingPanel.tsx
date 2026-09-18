import type { ReactNode, RefObject } from "react";
import { useLayoutEffect, useRef } from "react";
import "./floatingpanel.css";

export type FloatingPanelId = "notifications" | "voice" | "portables" | "skybox" | "friends";

export const FLOATING_PANEL_TITLES: Record<FloatingPanelId, string> = {
  notifications: "NOTIFICATIONS",
  voice: "NEARBY VOICE",
  portables: "Portable experiences",
  skybox: "NIGHT/DAY",
  friends: "Friends",
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

function useSidebarAnchor(id: string, ref: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    const measure = () => {
      if (!el.isConnected) return;
      el.style.maxHeight = "";
      const btn = sidebarButtonFor(id);
      const h = el.getBoundingClientRect().height;
      const { top, maxHeight } = anchorBeside(
        btn ? btn.getBoundingClientRect() : null,
        h,
        window.innerHeight,
      );
      el.style.top = `${top}px`;
      el.style.maxHeight = `${maxHeight}px`;
    };
    measure();
    window.addEventListener("resize", measure);
    const ro =
      typeof ResizeObserver === "function"
        ? new ResizeObserver(() => window.requestAnimationFrame(measure))
        : null;
    ro?.observe(el);
    return () => {
      window.removeEventListener("resize", measure);
      ro?.disconnect();
    };
  }, [id, ref]);
}

type FloatingPanelProps = {
  id: FloatingPanelId;
  onClose: () => void;
  title?: string;
  flush?: boolean;
  actions?: ReactNode;
  children: ReactNode;
};

export default function FloatingPanel({
  id,
  onClose,
  title,
  flush = false,
  actions,
  children,
}: FloatingPanelProps) {
  const ref = useRef<HTMLElement>(null);
  useSidebarAnchor(id, ref);
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
        <button type="button" className="fp__close" aria-label="Close" onClick={onClose}>
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
