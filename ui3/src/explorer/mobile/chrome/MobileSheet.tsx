import type { CSSProperties, PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useCallback, useRef, useState } from "react";
import { Close } from "../../../atoms/icons";
import type { MobileOrientation, MobileSheetSnap } from "./types";
import "./mobilesheet.css";

const DRAG_THRESHOLD = 50;
const PEEK_HEIGHT = 64;

const UP: Record<MobileSheetSnap, MobileSheetSnap> = { peek: "half", half: "full", full: "full" };
const DOWN: Record<MobileSheetSnap, MobileSheetSnap> = { full: "half", half: "peek", peek: "peek" };

type MobileSheetProps = {
  open?: boolean;
  orientation?: MobileOrientation;
  title?: string;
  snap?: MobileSheetSnap;
  defaultSnap?: MobileSheetSnap;
  halfRatio?: number;
  closeable?: boolean;
  onClose?: () => void;
  onSnapChange?: (snap: MobileSheetSnap) => void;
  children?: ReactNode;
};

export default function MobileSheet({
  open = false,
  orientation = "portrait",
  title = "",
  snap,
  defaultSnap = "half",
  halfRatio = 0.5,
  closeable = true,
  onClose,
  onSnapChange,
  children,
}: MobileSheetProps) {
  const [innerSnap, setInnerSnap] = useState<MobileSheetSnap>(defaultSnap);
  const [dragHeight, setDragHeight] = useState<number | null>(null);
  const drag = useRef<{ id: number; y: number; h: number } | null>(null);
  const current = snap ?? innerSnap;

  const apply = useCallback(
    (next: MobileSheetSnap) => {
      if (snap == null) setInnerSnap(next);
      if (next !== current) onSnapChange?.(next);
    },
    [current, onSnapChange, snap],
  );

  const onGrabDown = useCallback((e: ReactPointerEvent<HTMLDivElement>) => {
    if (drag.current) return;
    const panel = e.currentTarget.closest(".msh__panel");
    if (!(panel instanceof HTMLElement)) return;
    drag.current = { id: e.pointerId, y: e.clientY, h: panel.getBoundingClientRect().height };
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setDragHeight(drag.current.h);
  }, []);

  const onGrabMove = useCallback((e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d || d.id !== e.pointerId) return;
    const max = typeof window === "undefined" ? d.h : window.innerHeight;
    setDragHeight(Math.min(max, Math.max(PEEK_HEIGHT, d.h - (e.clientY - d.y))));
  }, []);

  const onGrabUp = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      const d = drag.current;
      if (!d || d.id !== e.pointerId) return;
      const dy = e.clientY - d.y;
      drag.current = null;
      setDragHeight(null);
      if (dy <= -DRAG_THRESHOLD) apply(UP[current]);
      else if (dy >= DRAG_THRESHOLD) {
        if (current === "peek" && closeable) onClose?.();
        else apply(DOWN[current]);
      }
    },
    [apply, closeable, current, onClose],
  );

  if (!open) return null;

  const style = {
    "--msh-half": `${Math.round(halfRatio * 100)}dvh`,
    ...(dragHeight == null ? null : { height: `${dragHeight}px` }),
  } as CSSProperties;

  return (
    <div className="msh" data-orientation={orientation} data-snap={current}>
      <button type="button" className="msh__scrim" aria-label="Close panel" onClick={onClose} />
      <div
        className="msh__panel"
        role="dialog"
        aria-modal="true"
        aria-label={title || "Panel"}
        data-dragging={dragHeight == null ? undefined : "true"}
        style={style}
      >
        {orientation === "portrait" ? (
          <div
            className="msh__grab"
            onPointerDown={onGrabDown}
            onPointerMove={onGrabMove}
            onPointerUp={onGrabUp}
            onPointerCancel={onGrabUp}
            onLostPointerCapture={onGrabUp}
          >
            <span className="msh__handle" aria-hidden="true" />
          </div>
        ) : null}

        <div className="msh__header">
          <h2 className="msh__title u-truncate">{title || "Panel"}</h2>
          {closeable ? (
            <button type="button" className="msh__close" aria-label="Close panel" onClick={onClose}>
              <Close strokeWidth={2.2} />
            </button>
          ) : null}
        </div>

        <div className="msh__body">{children}</div>
      </div>
    </div>
  );
}
