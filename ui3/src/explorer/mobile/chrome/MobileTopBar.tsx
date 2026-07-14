import type { ReactNode } from "react";
import { Badge } from "../../../atoms/primitives";
import type { MobileOrientation } from "./types";
import "./mobiletopbar.css";

function MenuGlyph() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
      <path d="M4 7h16M4 12h16M4 17h16" fill="none" stroke="currentColor"
        strokeWidth="2.2" strokeLinecap="round" />
    </svg>
  );
}

function ChatGlyph() {
  return (
    <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
      <path d="M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H9.5L5.5 19.5V16H6a2 2 0 0 1-2-2V6Z"
        fill="currentColor" />
    </svg>
  );
}

function HideGlyph() {
  return (
    <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
      <path d="M3 3l18 18M10.6 10.7a2 2 0 0 0 2.8 2.8M6.4 6.6C4.4 8 3 10 2.5 12c1.4 3.6 5.1 6 9.5 6 1.5 0 2.9-.3 4.1-.8M17.7 15.1c1.4-.9 2.5-2 3.1-3.1-1.4-3.6-5.1-6-9.5-6-.6 0-1.1 0-1.6.1"
        fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" />
    </svg>
  );
}

function PinGlyph() {
  return (
    <svg className="mtb__pin" viewBox="0 0 24 24" width="11" height="11" aria-hidden="true">
      <path d="M12 2c-3.9 0-7 3-7 6.9 0 4.6 7 12.1 7 12.1s7-7.5 7-12.1C19 5 15.9 2 12 2z"
        fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
      <circle cx="12" cy="9" r="2.4" fill="currentColor" />
    </svg>
  );
}

type MobileTopBarProps = {
  orientation?: MobileOrientation;
  place?: string;
  coords?: string;
  unread?: number;
  onMenu?: () => void;
  onChat?: () => void;
  onHideHud?: () => void;
  children?: ReactNode;
};

export default function MobileTopBar({
  orientation = "landscape",
  place = "",
  coords = "",
  unread = 0,
  onMenu,
  onChat,
  onHideHud,
  children,
}: MobileTopBarProps) {
  const unreadLabel = unread > 0 ? `Chat (${unread} unread)` : "Chat";
  return (
    <div className="mtb" data-orientation={orientation}>
      <button type="button" className="mtb__btn" aria-label="Open menu" onClick={onMenu}>
        <MenuGlyph />
      </button>

      <div className="mtb__place">
        <span className="mtb__name u-truncate">{place || "Unknown parcel"}</span>
        <span className="mtb__coords">
          <PinGlyph />
          {coords || "no coordinates"}
        </span>
      </div>

      {children}

      <div className="mtb__actions">
        <button type="button" className="mtb__btn" aria-label={unreadLabel} onClick={onChat}>
          <ChatGlyph />
          {unread > 0 ? (
            <span className="mtb__badge">
              <Badge tone="ruby">{unread > 99 ? "99+" : unread}</Badge>
            </span>
          ) : null}
        </button>
        {onHideHud ? (
          <button type="button" className="mtb__btn" aria-label="Hide interface" onClick={onHideHud}>
            <HideGlyph />
          </button>
        ) : null}
      </div>
    </div>
  );
}
