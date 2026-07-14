import { useState, useEffect, useRef } from "react";
import type { TeleportPayload } from "../../generated/bridge/TeleportPayload";
import { serviceBase } from "../../data/catalyst/client";
import { sendBridge } from "../../overlay/bridge";
import { useMinimapVisibility } from "../../overlay/minimapVisibility";
import ContextMenu from "../../components/ContextMenu";
import "./minimap.css";

const MENU = [
  "Jump to coordinates", "Copy coordinates", "Copy Link", "Share on Twitter",
] as const;

type MenuItem = (typeof MENU)[number];

const COORDS_RE = /^\s*(-?\d+)\s*,\s*(-?\d+)\s*$/;

function parseParcel(coords: string): { x: number; y: number } | null {
  if (typeof coords !== "string") return null;
  const m = coords.match(COORDS_RE);
  if (!m) return null;
  return { x: Number(m[1]), y: Number(m[2]) };
}

const PARCEL_SIZE = 16;
function parcelToTeleport(coords: string): TeleportPayload | null {
  const p = parseParcel(coords);
  if (!p) return null;
  return {
    x: p.x * PARCEL_SIZE + PARCEL_SIZE / 2,
    z: p.y * PARCEL_SIZE + PARCEL_SIZE / 2,
  };
}

function minimapSrc(parcel: { x: number; y: number }): string {
  return `${serviceBase("map")}/v1/map.png?center=${parcel.x},${parcel.y}&width=472&height=472&size=24`;
}

type MinimapProps = { place?: string; coords?: string; heading?: number };

export default function Minimap({ place = "", coords = "", heading }: MinimapProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const kebabRef = useRef<HTMLDivElement>(null);
  const { minimapHidden, userHidden, toggleUserHidden } = useMinimapVisibility();
  const parcel = parseParcel(coords);

  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: PointerEvent) => {
      const target = e.target;
      if (kebabRef.current && target instanceof Node && !kebabRef.current.contains(target)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("pointerdown", onDown, true);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
    };
  }, [menuOpen]);

  function jumpToCoords() {
    const dest = parcelToTeleport(coords);
    if (!dest) return;
    sendBridge("Teleport", dest);
  }

  function copyText(text: string) {
    if (!text) return;
    try {
      navigator.clipboard?.writeText(text);
    } catch {
    }
  }

  function placeUrl() {
    const p = parseParcel(coords);
    const pos = p ? `${p.x},${p.y}` : "0,0";
    return `https://decentraland.org/play/?position=${pos}`;
  }

  function onMenuItem(item: MenuItem) {
    switch (item) {
      case "Jump to coordinates":
        jumpToCoords();
        break;
      case "Copy coordinates":
        copyText(coords);
        break;
      case "Copy Link":
        copyText(placeUrl());
        break;
      case "Share on Twitter":
        if (typeof window !== "undefined")
          window.open(
            `https://twitter.com/intent/tweet?text=${encodeURIComponent(
              `Check out ${place} in Decentraland`,
            )}&url=${encodeURIComponent(placeUrl())}`,
            "_blank",
            "noopener,noreferrer",
          );
        break;
      default:
        break;
    }
    setMenuOpen(false);
  }

  if (minimapHidden) return null;

  if (userHidden) {
    return (
      <div className="mm__stage">
        <button
          type="button"
          className="mm__restore"
          onClick={toggleUserHidden}
          aria-label="Show map"
          title="Show map"
        >
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
            <path d="M12 2c-3.9 0-7 3-7 6.9 0 4.6 7 12.1 7 12.1s7-7.5 7-12.1C19 5 15.9 2 12 2z"
              fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
            <circle cx="12" cy="9" r="2.4" fill="currentColor" />
          </svg>
        </button>
      </div>
    );
  }

  return (
    <div className="mm__stage">
      <div className="mm">
        <div className="mm__header">
          <button className="mm__expand" aria-label="Expand map" title="Expand" data-sb-linkto="Explorer/Pages/Map">
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
              <path d="M7 8l4 4-4 4M13 8l4 4-4 4" fill="none" stroke="currentColor"
                strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
          <div className="mm__place">
            <span className="mm__name u-truncate">{place}</span>
            <span className="mm__coords">
              <svg className="mm__pin" viewBox="0 0 24 24" width="11" height="11" aria-hidden="true">
                <path d="M12 2c-3.9 0-7 3-7 6.9 0 4.6 7 12.1 7 12.1s7-7.5 7-12.1C19 5 15.9 2 12 2z"
                  fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
                <circle cx="12" cy="9" r="2.4" fill="currentColor" />
              </svg>
              {coords}
            </span>
          </div>

          <div className="mm__actions">
            <button
              type="button"
              className="mm__hide"
              onClick={toggleUserHidden}
              aria-label="Hide map"
              title="Hide map"
            >
              <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
                <path d="M5 12h14" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
              </svg>
            </button>
            <div className="mm__kebab-wrap" ref={kebabRef}>
              <button
                className="mm__kebab"
                title="Scene options"
                aria-label="Scene options"
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                onClick={() => setMenuOpen((o) => !o)}
              >
                <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
                  <circle cx="12" cy="5" r="1.7" fill="currentColor" />
                  <circle cx="12" cy="12" r="1.7" fill="currentColor" />
                  <circle cx="12" cy="19" r="1.7" fill="currentColor" />
                </svg>
              </button>

              {menuOpen && (
                <div className="mm__menu">
                  <ContextMenu
                    items={MENU.map((m) => ({ kind: "button", label: m, onClick: () => onMenuItem(m) }))}
                    onClose={() => setMenuOpen(false)}
                    autoFocus
                  />
                </div>
              )}
            </div>
          </div>
        </div>

        <button
          type="button"
          className="mm__map"
          data-sb-linkto="Explorer/Pages/Map"
          aria-label="Open full map"
          title="Open map"
        >
            <div className="mm__grid" aria-hidden="true" />
            {parcel && (
              <img className="mm__img" src={minimapSrc(parcel)} alt="" draggable={false} />
            )}
            <span className="mm__compass mm__compass--n" aria-hidden="true">N</span>
            <span className="mm__compass mm__compass--e" aria-hidden="true">E</span>
            <span className="mm__compass mm__compass--s" aria-hidden="true">S</span>
            <span className="mm__compass mm__compass--w" aria-hidden="true">W</span>
            <svg
              className="mm__player"
              viewBox="0 0 24 24"
              width="22"
              height="22"
              aria-hidden="true"
              style={heading == null ? undefined : { transform: `rotate(${heading}deg)` }}
            >
              <path d="M12 3l7 16-7-4-7 4 7-16z" fill="var(--brand)"
                stroke="#fff" strokeWidth="1.6" strokeLinejoin="round" />
            </svg>
          </button>
      </div>
    </div>
  );
}
