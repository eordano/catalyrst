import { useState, useEffect, useRef } from "react";
import type { ContextMenuItem } from "../../components/ContextMenu";
import { servicePath } from "../../data/catalyst/client";
import { safeCssUrl } from "../../data/cssUrl";
import { siteUrl } from "../../data/site";
import { truncateAddress } from "../../data/format";
import { useFriendPins } from "../../data/hooks/useFriendPins";
import { useBridgeState } from "../../overlay/bridge";
import { useMinimapVisibility } from "../../overlay/minimapVisibility";
import ContextMenu from "../../components/ContextMenu";
import { SceneFeedbackModal, SceneTipModal, useSceneOwner } from "../components/SceneOwnerActions";
import "./minimap.css";

const COORDS_RE = /^\s*(-?\d+)\s*,\s*(-?\d+)\s*$/;

function parseParcel(coords: string): { x: number; y: number } | null {
  if (typeof coords !== "string") return null;
  const m = coords.match(COORDS_RE);
  if (!m) return null;
  return { x: Number(m[1]), y: Number(m[2]) };
}

export function jumpUrl(coords: string): string {
  const p = parseParcel(coords);
  const pos = p ? `${p.x},${p.y}` : "0,0";
  const origin =
    typeof window !== "undefined" && window.location?.origin && window.location.origin !== "null"
      ? window.location.origin
      : siteUrl();
  return `${origin}/play/?position=${pos}`;
}

const MINIMAP_PX = 472;
const MINIMAP_PARCEL_PX = 8;
const MINIMAP_PCT_PER_PARCEL = (MINIMAP_PARCEL_PX / MINIMAP_PX) * 100;
const MINIMAP_VIEW_RADIUS_PCT = 44;

function minimapSrc(parcel: { x: number; y: number }): string {
  return `${servicePath("map")}/v1/map.png?center=${parcel.x},${parcel.y}&width=${MINIMAP_PX}&height=${MINIMAP_PX}&size=${MINIMAP_PARCEL_PX}`;
}

function minimapDotPos(
  parcel: { x: number; y: number },
  fx: number,
  fy: number,
): { left: number; top: number } | null {
  const left = 50 + (fx - parcel.x) * MINIMAP_PCT_PER_PARCEL;
  const top = 50 - (fy - parcel.y) * MINIMAP_PCT_PER_PARCEL;
  if (Math.hypot(left - 50, top - 50) > MINIMAP_VIEW_RADIUS_PCT) return null;
  return { left, top };
}

function MenuLabel({ main, hint }: { main: string; hint: string }) {
  return (
    <span className="mm__mlabel">
      <span className="mm__mmain">{main}</span>
      <span className="mm__mhint">{hint}</span>
    </span>
  );
}

type OwnerModal = "feedback" | "tip" | null;

type MinimapProps = { place?: string; coords?: string; heading?: number; inline?: boolean };

export default function Minimap({ place = "", coords = "", heading, inline = false }: MinimapProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [ownerModal, setOwnerModal] = useState<OwnerModal>(null);
  const kebabRef = useRef<HTMLDivElement>(null);
  const { minimapHidden, userHidden, toggleUserHidden } = useMinimapVisibility();
  const realm = useBridgeState((s) => s.scene.realm);
  const shown = !(minimapHidden || userHidden);
  const owner = useSceneOwner(coords, realm, menuOpen && shown);
  const parcel = parseParcel(coords);
  const parcelText = parcel ? `${parcel.x},${parcel.y}` : "";
  const friendPins = useFriendPins();

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

  function copyText(text: string) {
    if (!text) return;
    try {
      navigator.clipboard?.writeText(text);
    } catch {
    }
  }

  function pick(action: () => void) {
    setMenuOpen(false);
    action();
  }

  const ownerHint = owner.address
    ? `owner ${truncateAddress(owner.address)}`
    : owner.loading
      ? "finding the owner\u{2026}"
      : owner.world
        ? "world owner unknown"
        : "owner unknown";

  const menuItems: ContextMenuItem[] = [
    {
      kind: "button",
      label: <MenuLabel main="Copy coordinates" hint={parcelText || "no parcel"} />,
      disabled: !parcel,
      onClick: () => pick(() => copyText(parcelText)),
    },
    {
      kind: "button",
      label: <MenuLabel main="Copy jump link" hint={jumpUrl(coords)} />,
      disabled: !parcel,
      onClick: () => pick(() => copyText(jumpUrl(coords))),
    },
    { kind: "separator" },
    {
      kind: "button",
      label: <MenuLabel main="Send feedback to scene owner" hint={ownerHint} />,
      disabled: !owner.address,
      onClick: () => pick(() => setOwnerModal("feedback")),
    },
    {
      kind: "button",
      label: <MenuLabel main="Send tip to scene owner" hint={ownerHint} />,
      disabled: !owner.address,
      onClick: () => pick(() => setOwnerModal("tip")),
    },
  ];

  if (!shown) return null;

  const modalScene = place || owner.sceneTitle || "";

  return (
    <div className={"mm__stage" + (inline ? " mm__stage--inline" : "")}>
      <div className="mm">
        {!inline && <div className="mm__header">
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
                  <ContextMenu items={menuItems} onClose={() => setMenuOpen(false)} autoFocus />
                </div>
              )}
            </div>
          </div>
        </div>}

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
            {parcel &&
              friendPins.map((f) => {
                const pos = minimapDotPos(parcel, f.x, f.y);
                if (!pos) return null;
                const bg = safeCssUrl(f.picture);
                return (
                  <span
                    key={f.address}
                    className="mm__friend"
                    style={{
                      left: `${pos.left}%`,
                      top: `${pos.top}%`,
                      ...(bg ? { backgroundImage: bg } : null),
                    }}
                    title={f.name}
                    aria-hidden="true"
                  />
                );
              })}
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

      {ownerModal === "feedback" && owner.address && (
        <SceneFeedbackModal
          owner={owner.address}
          sceneTitle={modalScene}
          coords={parcelText || coords}
          onClose={() => setOwnerModal(null)}
        />
      )}
      {ownerModal === "tip" && owner.address && (
        <SceneTipModal
          owner={owner.address}
          sceneTitle={modalScene}
          coords={parcelText || coords}
          onClose={() => setOwnerModal(null)}
        />
      )}
    </div>
  );
}
