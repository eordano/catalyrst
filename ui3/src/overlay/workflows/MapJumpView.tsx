import type { CSSProperties } from "react";
import { useCallback, useRef } from "react";

import ExploreChrome from "../../explorer/frames/ExploreChrome";
import SearchField from "../../atoms/SearchField";
import Checkbox from "../../atoms/Checkbox";
import Spinner from "../../atoms/Spinner";
import { useDialogKeys } from "../../components/useDialogKeys";
import "../../explorer/pages/map.css";
import "../../explorer/pages/mapfilters.css";
import "./mapjumpview.css";

type MjPin = {
  id: string;
  name: string;
  coords: string;
  x: number;
  y: number;
  category: string;
  /** null renders an em dash: no headcount was read, which is not zero visitors */
  users: number | null;
  rating: number;
  live: boolean;
  featured: boolean;
  creator: string;
  image: string | null;
};

type MjCategory = { key: string; label: string };

type MapJumpViewProps<P extends MjPin> = {
  value?: string;
  step?: string;
  source?: string;
  /**
   * Non-null when the destination list could not be read. The map then shows
   * the reason and no pins: a pin here is a teleport target, so a substituted
   * one would send the player to a coordinate nobody published.
   */
  unavailableReason?: string | null;
  pins?: P[];
  categories?: readonly MjCategory[];
  filter?: string;
  pin?: P | null;
  setHome?: boolean;
  error?: string;
  jumpUrl?: string;
  onTab?: (id: string) => void;
  onClose?: () => void;
  onSearch?: (value: string) => void;
  onZoomIn?: () => void;
  onZoomOut?: () => void;
  onRecenter?: () => void;
  onLayers?: () => void;
  onSelectPin?: (pin: P) => void;
  onFilter?: (key: string) => void;
  onClear?: () => void;
  onConfirm?: () => void;
  onToggleHome?: () => void;
  onJump?: () => void;
  onBack?: () => void;
  onRetry?: () => void;
};

export default function MapJumpView<P extends MjPin>({
  value = "browsing",
  step = "map",
  source = "unknown",
  unavailableReason = null,
  pins = [],
  categories = [],
  filter = "all",
  pin = null,
  setHome = false,
  error = undefined,
  jumpUrl = undefined,
  onTab = undefined,
  onClose = undefined,
  onSearch = undefined,
  onZoomIn = undefined,
  onZoomOut = undefined,
  onRecenter = undefined,
  onLayers = undefined,
  onSelectPin = undefined,
  onFilter = undefined,
  onClear = undefined,
  onConfirm = undefined,
  onToggleHome = undefined,
  onJump = undefined,
  onBack = undefined,
  onRetry = undefined,
}: MapJumpViewProps<P>) {
  const showInfo = value === "selected" || value === "confirming";
  const teleporting = value === "jumping";
  const arrived = value === "done";
  const failed = value === "error";

  const infoRef = useRef<HTMLDivElement>(null);
  const closeInfo = useCallback(() => {
    if (!showInfo) return;
    if (value === "confirming") onBack?.();
    else onClear?.();
  }, [showInfo, value, onBack, onClear]);
  useDialogKeys(infoRef, closeInfo);

  return (
    <ExploreChrome active="map" onTab={onTab} onClose={onClose}>
      <div className="map__shell" data-step={step} data-source={source}>
        <div
          className="map__tiles"
          onClick={() => {
            if (value === "selected") onClear?.();
          }}
        >
          <div className="map__grid" />
          <div className="map__roads" />
          <div className="map__district map__district--plaza" style={{ left: "46%", top: "44%", width: "8%", height: "9%" }} />
          <div className="map__district map__district--purple" style={{ left: "26%", top: "28%", width: "10%", height: "12%" }} />
          <div className="map__district map__district--blue" style={{ left: "60%", top: "56%", width: "13%", height: "11%" }} />
          <div className="map__district map__district--pink" style={{ left: "58%", top: "23%", width: "9%", height: "8%" }} />

          <div className="map__player" style={{ ...pinPos("0,0") }} aria-label="Your location">
            <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
              <path d="M12 3 20 21l-8-5-8 5 8-18z" fill="#fff" />
            </svg>
          </div>

          {pins.map((p) => (
            <button
              key={p.id}
              type="button"
              className={
                "map__pin map__pin--" +
                pinKind(p) +
                (pin?.id === p.id ? " is-selected" : "")
              }
              style={{ ...pinPos(p.coords) }}
              onClick={(e) => {
                e.stopPropagation();
                onSelectPin?.(p);
              }}
              aria-label={p.name}
            >
              <PinIcon />
            </button>
          ))}
        </div>

        <div className="map__catbar">
          <div className="map__cats" role="tablist" aria-label="Place categories">
            {categories.map((c) => (
              <button
                key={c.key}
                type="button"
                role="tab"
                aria-selected={c.key === filter}
                className={"map__catpill" + (c.key === filter ? " is-on" : "")}
                onClick={() => onFilter?.(c.key)}
              >
                <span className="map__catdot" aria-hidden="true" style={{ background: catColor(c.key) }} />
                {c.label}
              </button>
            ))}
          </div>
          <div className="map__search">
            <SearchField placeholder="Search" value="" onChange={onSearch} />
          </div>
        </div>

        <div className="map__zoom">
          <div className="map__zgroup">
            <button className="map__zbtn" aria-label="Zoom in" onClick={onZoomIn}>+</button>
            <button className="map__zbtn" aria-label="Zoom out" onClick={onZoomOut}>&#x2212;</button>
          </div>
          <button className="map__zbtn map__locate" aria-label="Recenter" onClick={onRecenter}>&#x2295;</button>
          <button className="map__zbtn map__layers" aria-label="Map layers" onClick={onLayers}>&#x29C9;</button>
        </div>

        <div className="map__credit">Powered by the Decentraland Foundation</div>

        {unavailableReason !== null && (
          <div className="map__unavailable" role="alert">
            <p className="map__unavailabletitle">
              We couldn't load the destination list
            </p>
            <p className="map__unavailablesub">
              {unavailableReason || "The places list did not answer."}
            </p>
            <p className="map__unavailablenote">
              No pins are shown. A stand-in destination would look exactly like a
              real one and teleport you to a place nobody published. Reload to
              try again.
            </p>
          </div>
        )}

        {showInfo && pin && (
          <div
            className="map__info"
            role="dialog"
            aria-modal="true"
            aria-label={`Destination ${pin.name}`}
            tabIndex={-1}
            ref={infoRef}
          >
            <div
              className="map__infothumb"
              style={{
                "--hue": (pin.x * 5 + 200) % 360,
                ...(pin.image
                  ? { backgroundImage: `url(${pin.image})`, backgroundSize: "cover", backgroundPosition: "center" }
                  : null),
              } as CSSProperties}
            >
              {pin.live && <span className="map__infolive">&#x25CF; LIVE</span>}
              <button
                className="map__infoclose"
                onClick={closeInfo}
                aria-label="Close"
              >
                &#xD7;
              </button>
            </div>
            <div className="map__infobody">
              <div className="map__infoname">{pin.name}</div>
              {pin.creator && (
                <div className="map__infocreator">
                  created by <b>{pin.creator}</b>
                </div>
              )}
              <div className="map__inforow">
                <span className="map__infostat"><b>{pin.coords}</b><span>LOCATION</span></span>
                <span className="map__infostat"><b>{pin.rating}%</b><span>RATING</span></span>
                <span className="map__infostat"><b>{pin.users ?? "\u2014"}</b><span>VISITORS</span></span>
              </div>

              {value === "selected" && (
                <div className="map__infoactions">
                  <button
                    type="button"
                    className="map__jump"
                    onClick={onConfirm}
                  >
                    jump in
                  </button>
                </div>
              )}

              {value === "confirming" && (
                <div className="map__confirm">
                  <div className="map__home">
                    <Checkbox checked={setHome} onChange={() => onToggleHome?.()}>
                      Set as my home
                    </Checkbox>
                  </div>
                  <div className="map__infoactions">
                    <button type="button" className="map__nav" onClick={onBack}>
                      back
                    </button>
                    <button type="button" className="map__jump" onClick={onJump}>
                      confirm &amp; jump
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}

        {(teleporting || arrived || failed) && (
          <div className="map__teleport" role="status" aria-live="polite">
            <div className="map__teleportcard">
              {teleporting && (
                <>
                  <div className="map__teleportspinner">
                    <Spinner size={40} color="var(--explore-orange, #ff7a3d)" aria-hidden="true" />
                  </div>
                  <p className="map__teleporttitle">Teleporting&#x2026;</p>
                  <p className="map__teleportsub">
                    Taking you to <b>{pin?.name ?? "your destination"}</b>{" "}
                    ({pin?.coords})
                  </p>
                </>
              )}
              {arrived && (
                <>
                  <p className="map__teleporttitle">You've arrived</p>
                  <p className="map__teleportsub">
                    Welcome to <b>{pin?.name}</b> ({pin?.coords}).
                  </p>
                  <p className="map__teleportnote">
                    Engine teleport when the bridge is present; otherwise deep link:{" "}
                    <code>{jumpUrl}</code>
                  </p>
                </>
              )}
              {failed && (
                <>
                  <p className="map__teleporttitle">Teleport failed</p>
                  <p className="map__teleportsub">{error}</p>
                  <button type="button" className="map__jump" onClick={onRetry}>
                    retry
                  </button>
                </>
              )}
            </div>
          </div>
        )}
      </div>
    </ExploreChrome>
  );
}

const WORLD_MIN = -160;
const WORLD_MAX = 160;
const WORLD_SPAN = WORLD_MAX - WORLD_MIN;

function pinPos(coords: string): CSSProperties {
  const [xs, ys] = (coords || "0,0").split(",");
  const x = clampNum(Number.parseInt((xs ?? "0").trim(), 10));
  const y = clampNum(Number.parseInt((ys ?? "0").trim(), 10));
  const left = ((x - WORLD_MIN) / WORLD_SPAN) * 100;
  const top = (1 - (y - WORLD_MIN) / WORLD_SPAN) * 100;
  return { left: `${pct(left)}%`, top: `${pct(top)}%` };
}

function clampNum(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.max(WORLD_MIN, Math.min(WORLD_MAX, n));
}
function pct(n: number): number {
  return Math.max(2, Math.min(98, Math.round(n * 10) / 10));
}

function pinKind(p: MjPin): string {
  if (p.live) return "live";
  if (p.category === "poi") return "poi";
  if (p.featured) return "fav";
  return "place";
}

const CAT_COLOR: Record<string, string> = {
  all: "#ffffff",
  live: "#ff7a18",
  poi: "#ffb019",
  minigames: "#a14bff",
  people: "#5db0ff",
};
function catColor(key: string): string {
  return CAT_COLOR[key] ?? "#ffffff";
}

function PinIcon() {
  return (
    <svg viewBox="0 0 24 32" width="24" height="32" aria-hidden="true">
      <path
        d="M12 0C5.4 0 0 5.2 0 11.7 0 20 12 32 12 32s12-12 12-20.3C24 5.2 18.6 0 12 0z"
        className="map__pindrop"
      />
      <circle cx="12" cy="11.5" r="4.6" fill="#fff" />
    </svg>
  );
}
