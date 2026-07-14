import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { MobileAction, MobileGlyph, MobileOrientation } from "./types";
import "./mobileactioncluster.css";

export const TOUCH_ACTIONS: MobileAction[] = [
  { id: "jump", label: "Jump", glyph: "jump" },
  { id: "primary", label: "Primary action", glyph: "hand" },
  { id: "secondary", label: "Secondary action", glyph: "spark" },
];

const GLYPHS: Record<MobileGlyph, ReactNode> = {
  jump: (
    <svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true">
      <path d="M12 3.5 6.8 9M12 3.5 17.2 9M12 3.5v10" fill="none" stroke="currentColor"
        strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M5.5 18.5h13" fill="none" stroke="currentColor"
        strokeWidth="2.2" strokeLinecap="round" />
    </svg>
  ),
  hand: (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path d="M10 12V5.4a1.4 1.4 0 0 1 2.8 0V11m0-1.2a1.4 1.4 0 0 1 2.8 0V12m0-1a1.4 1.4 0 0 1 2.8 0v4.4a5 5 0 0 1-5 5h-1.9a4.6 4.6 0 0 1-3.6-1.8L4 14.4a1.4 1.4 0 0 1 2-2l2 2"
        fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  spark: (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" fill="currentColor" />
    </svg>
  ),
  emote: (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <circle cx="12" cy="12" r="8.6" fill="none" stroke="currentColor" strokeWidth="1.9" />
      <circle cx="9.4" cy="10.2" r="1.1" fill="currentColor" />
      <circle cx="14.6" cy="10.2" r="1.1" fill="currentColor" />
      <path d="M8.6 14.2c.9 1.2 2 1.8 3.4 1.8s2.5-.6 3.4-1.8" fill="none" stroke="currentColor"
        strokeWidth="1.9" strokeLinecap="round" />
    </svg>
  ),
  chat: (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path d="M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H9.5L5.5 19.5V16H6a2 2 0 0 1-2-2V6Z"
        fill="currentColor" />
    </svg>
  ),
  camera: (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path d="M9 4 7.5 6H5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-2.5L15 4H9Zm3 5a4.2 4.2 0 1 1 0 8.4A4.2 4.2 0 0 1 12 9Z"
        fill="currentColor" />
    </svg>
  ),
};

type ActionButtonProps = {
  action: MobileAction;
  main?: boolean;
  onPress?: (id: string) => void;
  onRelease?: (id: string) => void;
};

function ActionButton({ action, main, onPress, onRelease }: ActionButtonProps) {
  const pointerId = useRef<number | null>(null);
  const [held, setHeld] = useState(false);
  const releaseRef = useRef(onRelease);
  releaseRef.current = onRelease;

  const end = useCallback(
    (e: ReactPointerEvent<HTMLButtonElement>) => {
      if (pointerId.current !== e.pointerId) return;
      pointerId.current = null;
      setHeld(false);
      releaseRef.current?.(action.id);
    },
    [action.id],
  );

  const begin = useCallback(
    (e: ReactPointerEvent<HTMLButtonElement>) => {
      if (pointerId.current !== null) return;
      pointerId.current = e.pointerId;
      e.currentTarget.setPointerCapture?.(e.pointerId);
      e.preventDefault();
      setHeld(true);
      onPress?.(action.id);
    },
    [action.id, onPress],
  );

  useEffect(
    () => () => {
      if (pointerId.current !== null) {
        pointerId.current = null;
        releaseRef.current?.(action.id);
      }
    },
    [action.id],
  );

  const glyph = action.glyph ? GLYPHS[action.glyph] : null;
  return (
    <button
      type="button"
      className={"mac__btn" + (main ? " mac__btn--main" : "") + (held ? " is-held" : "")}
      aria-label={action.label}
      aria-pressed={held}
      onPointerDown={begin}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
    >
      {action.iconUrl ? <img className="mac__icon" src={action.iconUrl} alt="" /> : glyph}
    </button>
  );
}

type MobileActionClusterProps = {
  orientation?: MobileOrientation;
  actions?: MobileAction[];
  mainAction?: string;
  hidden?: boolean;
  onActionPress?: (id: string) => void;
  onActionRelease?: (id: string) => void;
};

export default function MobileActionCluster({
  orientation = "landscape",
  actions = TOUCH_ACTIONS,
  mainAction = "jump",
  hidden = false,
  onActionPress,
  onActionRelease,
}: MobileActionClusterProps) {
  if (hidden) return null;
  const visible = actions.filter((a) => !a.hidden);
  if (visible.length === 0) return null;
  const main = visible.find((a) => a.id === mainAction);
  const combos = visible.filter((a) => a !== main);
  return (
    <div className="mac" data-orientation={orientation} role="group" aria-label="Touch actions">
      {combos.length > 0 ? (
        <div className="mac__combos">
          {combos.map((a) => (
            <ActionButton key={a.id} action={a} onPress={onActionPress} onRelease={onActionRelease} />
          ))}
        </div>
      ) : null}
      {main ? (
        <ActionButton action={main} main onPress={onActionPress} onRelease={onActionRelease} />
      ) : null}
    </div>
  );
}
