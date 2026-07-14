import type { MobileOrientation } from "./types";
import "./mobileminimappuck.css";

type MobileMinimapPuckProps = {
  orientation?: MobileOrientation;
  place?: string;
  coords?: string;
  heading?: number;
  mapSrc?: string | null;
  collapsed?: boolean;
  onExpand?: () => void;
  onToggleCollapsed?: () => void;
};

export default function MobileMinimapPuck({
  orientation = "landscape",
  place = "",
  coords = "",
  heading,
  mapSrc,
  collapsed = false,
  onExpand,
  onToggleCollapsed,
}: MobileMinimapPuckProps) {
  const label = place || coords || "Unknown parcel";

  if (collapsed || orientation === "portrait") {
    return (
      <div className="mmp" data-orientation={orientation} data-collapsed={collapsed || undefined}>
        <button
          type="button"
          className="mmp__chip"
          onClick={onExpand}
          aria-label={`Open map \u{2014} ${label}`}
          data-sb-linkto="Explorer/Pages/Map"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
            <path d="M12 2c-3.9 0-7 3-7 6.9 0 4.6 7 12.1 7 12.1s7-7.5 7-12.1C19 5 15.9 2 12 2z"
              fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
            <circle cx="12" cy="9" r="2.4" fill="currentColor" />
          </svg>
          <span className="mmp__chiptext u-truncate">{coords || "no coordinates"}</span>
        </button>
        {onToggleCollapsed && collapsed ? (
          <button type="button" className="mmp__toggle" onClick={onToggleCollapsed} aria-label="Show map">
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
              <path d="M6 15l6-6 6 6" fill="none" stroke="currentColor" strokeWidth="2.2"
                strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        ) : null}
      </div>
    );
  }

  return (
    <div className="mmp" data-orientation={orientation}>
      <button
        type="button"
        className="mmp__puck"
        onClick={onExpand}
        aria-label={`Open map \u{2014} ${label}`}
        data-sb-linkto="Explorer/Pages/Map"
      >
        <span className="mmp__grid" aria-hidden="true" />
        {mapSrc ? <img className="mmp__img" src={mapSrc} alt="" draggable={false} /> : null}
        <span className="mmp__north" aria-hidden="true">N</span>
        <svg
          className="mmp__player"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          aria-hidden="true"
          style={heading == null ? undefined : { transform: `rotate(${heading}deg)` }}
        >
          <path d="M12 3l7 16-7-4-7 4 7-16z" fill="var(--brand)" stroke="#fff"
            strokeWidth="1.6" strokeLinejoin="round" />
        </svg>
      </button>
      <span className="mmp__coords">{coords || "no coordinates"}</span>
      {onToggleCollapsed ? (
        <button type="button" className="mmp__toggle" onClick={onToggleCollapsed} aria-label="Hide map">
          <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
            <path d="M5 12h14" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" />
          </svg>
        </button>
      ) : null}
    </div>
  );
}
