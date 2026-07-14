import type { QueryClient } from "@tanstack/react-query";
import type {
  CSSProperties,
  MouseEvent as ReactMouseEvent,
  PointerEvent as ReactPointerEvent,
} from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import JumpLoading, { useJump } from "../../explorer/components/JumpLoading";

import SearchField from "../../atoms/SearchField";
import PlaceDetail from "../../explorer/pages/PlaceDetail";
import WorldVisitModal from "../../components/WorldVisitModal";
import { sendBridge, useBridgeState } from "../../overlay/bridge";
import { usePlaces, usePlace, useCategories } from "../../data/hooks/usePlaces";
import { useFriendPins, type FriendPin } from "../../data/hooks/useFriendPins";
import { usePlaceSearch } from "../../data/hooks/usePlaceSearch";
import { serviceBase } from "../../data/catalyst/client";
import { safeCssUrl } from "../../data/cssUrl";
import { toPlaceDetail, coordsToPercent } from "../../data/catalyst/places";
import { fetchPlaces, fetchCategories } from "../../data/catalyst/placesSchema";
import type { PlaceView } from "../../data/catalyst/places";
import { qk, STALE } from "../../data/queryKeys";
import "../../explorer/pages/map.css";

type VarStyle = CSSProperties & { [k: `--${string}`]: string | number };
const cssVars = (s: VarStyle): CSSProperties => s;

type ViewMetrics = { w: number; h: number; square: number };
type Metrics = { w: number; h: number; cx: number; cy: number; square: number };
type SlippyView = { w: number; h: number; square: number; panX: number; panY: number };
type Tile = { key: string; src: string; left: number; top: number; size: number };
type DragState = { sx: number; sy: number; px: number; py: number; moved: boolean };
type Pan = { x: number; y: number };

const LIST_PARAMS = { limit: 100, order_by: "most_active", order: "desc" };
const POI_PARAMS = { limit: 100, categories: "poi" };

const SAT_BASE = serviceBase("satellite");
const SAT_WORLD_MIN = -256;
const SAT_WORLD_SPAN = 512;
const SAT_GRID_MIN = -170;
const SAT_GRID_SPAN = 340;
const SAT_TILE = (z: number, x: number, y: number) => `${SAT_BASE}/${z}/${x}/${y}.png`;
const MINIMAP_SRC = `${serviceBase("map")}/v1/minimap.png`;

function slippyTilesFor(zoom: number, view: SlippyView | null): Tile[] {
  if (!SAT_BASE) return [];
  const z = Math.max(1, Math.min(7, Math.round(2.6 + Math.log2(zoom))));
  const n = 1 << z;
  const span = SAT_WORLD_SPAN / n;

  let west;
  let east;
  let south;
  let north;
  if (view && view.square > 0) {
    const scaled = view.square * zoom;
    const u0 = 0.5 + (-view.w / 2 - view.panX) / scaled;
    const u1 = 0.5 + (view.w / 2 - view.panX) / scaled;
    const v0 = 0.5 + (-view.h / 2 - view.panY) / scaled;
    const v1 = 0.5 + (view.h / 2 - view.panY) / scaled;
    west = SAT_GRID_MIN + u0 * SAT_GRID_SPAN;
    east = SAT_GRID_MIN + u1 * SAT_GRID_SPAN;
    north = SAT_GRID_MIN + SAT_GRID_SPAN - v0 * SAT_GRID_SPAN;
    south = SAT_GRID_MIN + SAT_GRID_SPAN - v1 * SAT_GRID_SPAN;
  } else {
    west = -170 / zoom;
    east = 170 / zoom;
    south = -170 / zoom;
    north = 170 / zoom;
  }

  const xIdx = (w: number) => Math.floor((w - SAT_WORLD_MIN) / span);
  const yIdx = (w: number) => Math.floor((SAT_WORLD_MIN + SAT_WORLD_SPAN - w) / span);
  const loX = Math.max(0, xIdx(west) - 1);
  const hiX = Math.min(n - 1, xIdx(east) + 1);
  const loY = Math.max(0, yIdx(north) - 1);
  const hiY = Math.min(n - 1, yIdx(south) + 1);

  const tiles: Tile[] = [];
  for (let x = loX; x <= hiX; x++) {
    const wx0 = SAT_WORLD_MIN + x * span;
    for (let y = loY; y <= hiY; y++) {
      const wyTop = SAT_WORLD_MIN + SAT_WORLD_SPAN - y * span;
      tiles.push({
        key: `${z}/${x}/${y}`,
        src: SAT_TILE(z, x, y),
        left: ((wx0 - SAT_GRID_MIN) / SAT_GRID_SPAN) * 100,
        top: ((SAT_GRID_MIN + SAT_GRID_SPAN - wyTop) / SAT_GRID_SPAN) * 100,
        size: (span / SAT_GRID_SPAN) * 100,
      });
    }
  }
  return tiles;
}

export function prefetch(queryClient: QueryClient) {
  try {
    queryClient.prefetchQuery({
      queryKey: qk.places(LIST_PARAMS),
      queryFn: ({ signal }) => fetchPlaces(LIST_PARAMS, { signal }),
      staleTime: STALE.places,
    });
    queryClient.prefetchQuery({
      queryKey: qk.places(POI_PARAMS),
      queryFn: ({ signal }) => fetchPlaces(POI_PARAMS, { signal }),
      staleTime: STALE.places,
    });
    queryClient.prefetchQuery({
      queryKey: qk.categories(),
      queryFn: ({ signal }) => fetchCategories({ signal }),
      staleTime: STALE.categories,
    });
  } catch {
  }
}

function PinIcon({ kind, picture }: { kind: string; picture?: string | null }) {
  if (kind === "friend") {
    const bg = safeCssUrl(picture);
    return (
      <span
        className="map__pinfriend"
        style={bg ? { backgroundImage: bg } : undefined}
      />
    );
  }
  return (
    <svg viewBox="0 0 24 32" width="24" height="32" aria-hidden="true">
      <path
        d="M12 0C5.4 0 0 5.2 0 11.7 0 20 12 32 12 32s12-12 12-20.3C24 5.2 18.6 0 12 0z"
        className="map__pindrop"
      />
      {kind === "poi" ? (
        <path
          d="M12 6.3 13.29 9.72 16.95 9.89 14.09 12.18 15.06 15.71 12 13.7 8.94 15.71 9.91 12.18 7.05 9.89 10.71 9.72Z"
          fill="#fff"
        />
      ) : (
        <circle cx="12" cy="11.5" r="4.6" fill="#fff" />
      )}
    </svg>
  );
}

const PARCEL_SIZE = 16;

function teleportTo(view: PlaceView | null) {
  if (!view || typeof window === "undefined") return;
  if (view.world && view.worldName) {
    sendBridge("ChangeRealm", { realm: view.worldName });
    return;
  }
  const px = Number(view.x);
  const py = Number(view.y);
  if (!Number.isFinite(px) || !Number.isFinite(py)) return;
  sendBridge("Teleport", {
    x: px * PARCEL_SIZE + PARCEL_SIZE / 2,
    z: py * PARCEL_SIZE + PARCEL_SIZE / 2,
  });
}

const SIDEBAR_SORTS: { key: "most_active" | "like_score" | "created_at"; label: string }[] = [
  { key: "most_active", label: "MOST ACTIVE" },
  { key: "like_score", label: "BEST RATED" },
  { key: "created_at", label: "NEWEST" },
];

function CategorySidebar({
  label,
  color,
  sort,
  setSort,
  places,
  loading,
  onClose,
  onSelect,
}: {
  label: string;
  color: string;
  sort: string;
  setSort: (s: "most_active" | "like_score" | "created_at") => void;
  places: PlaceView[];
  loading: boolean;
  onClose: () => void;
  onSelect: (p: PlaceView) => void;
}) {
  return (
    <aside className="map__sidebar">
      <div className="map__sidebarhead">
        <span className="map__sidebardot" style={{ background: color }} aria-hidden="true" />
        <span className="map__sidebartitle">{label}</span>
        <button type="button" className="map__sidebarclose" aria-label="Close" onClick={onClose}>&#xD7;</button>
      </div>
      <div className="map__sidebarsorts">
        {SIDEBAR_SORTS.map((s) => (
          <button
            key={s.key}
            type="button"
            className={"map__sidebarsort" + (sort === s.key ? " is-on" : "")}
            onClick={() => setSort(s.key)}
          >
            {s.label}
          </button>
        ))}
      </div>
      <div className="map__sidebarlist">
        {loading && <div className="map__sidebarmsg">Loading&#x2026;</div>}
        {!loading && places.length === 0 && <div className="map__sidebarmsg">No scenes found.</div>}
        {!loading &&
          places.map((p) => (
            <button key={p.id} type="button" className="map__sidebarcard" onClick={() => onSelect(p)}>
              <div
                className="map__sidebarcardimg"
                style={{ "--hue": p.hue, ...(p.image ? { backgroundImage: `url("${p.image}")` } : null) } as CSSProperties}
              />
              <div className="map__sidebarcardbody">
                <div className="map__sidebarcardtitle">{p.title}</div>
                <div className="map__sidebarcardcreator">created by {p.creator}</div>
                <div className="map__sidebarcardstats">
                  <span>&#x1F44D; {p.rating}%</span>
                  <span>&#x1F464; {p.players ?? "\u{2014}"}</span>
                </div>
              </div>
              <span className="map__sidebarcardchevron">&#x203A;</span>
            </button>
          ))}
      </div>
    </aside>
  );
}

export default function MapPanel() {
  const sceneCoords = useBridgeState((s) => s.scene.coords);
  const placesQ = usePlaces(LIST_PARAMS);
  const poisQ = usePlaces(POI_PARAMS);
  const catsQ = useCategories();

  const navigate = useNavigate();
  const [cat, setCat] = useState("ALL");
  const [search, setSearch] = useState("");
  const { jumping, stalled, beginJump, cancelJump, confirmJump } = useJump(() => navigate("/"));
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selFriendAddr, setSelFriendAddr] = useState<string | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState<Pan>({ x: 0, y: 0 });
  const [animate, setAnimate] = useState(true);
  const [grabbing, setGrabbing] = useState(false);
  const [box, setBox] = useState<ViewMetrics | null>(null);
  const [sidebarSort, setSidebarSort] = useState<"most_active" | "like_score" | "created_at">("most_active");
  const [confirmWorld, setConfirmWorld] = useState<{ realm: string; title?: string } | null>(null);
  const { placeHits: searchPlaceHits, worldHits: searchWorldHits } = usePlaceSearch(search);

  const ZOOM_MAX = 4;
  const ZOOM_STEP = 0.25;
  const coverZoom = (m?: ViewMetrics | Metrics | null): number => {
    const mm = m ?? box;
    if (!mm || !mm.square) return 1;
    return Math.max(mm.w, mm.h) / mm.square;
  };
  const clampZoom = (z: number, m?: ViewMetrics | Metrics | null) => {
    const cover = coverZoom(m);
    const r = Math.round(Math.min(ZOOM_MAX, z) * 100) / 100;
    return r < cover ? cover : r;
  };

  const shellRef = useRef<HTMLDivElement | null>(null);
  const zoomRef = useRef(zoom);
  const panRef = useRef(pan);
  zoomRef.current = zoom;
  panRef.current = pan;
  const dragRef = useRef<DragState | null>(null);
  const draggedRef = useRef(false);
  const wheelEndRef = useRef(0);

  const metrics = (): Metrics | null => {
    const el = shellRef.current;
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return {
      w: r.width,
      h: r.height,
      cx: r.left + r.width / 2,
      cy: r.top + r.height / 2,
      square: Math.min(r.width, r.height),
    };
  };

  const clampPan = (px: number, py: number, z: number, m?: Metrics | null): Pan => {
    const mm = m || metrics();
    if (!mm) return { x: px, y: py };
    const maxX = Math.max(0, (mm.square * z - mm.w) / 2);
    const maxY = Math.max(0, (mm.square * z - mm.h) / 2);
    return {
      x: Math.max(-maxX, Math.min(maxX, px)),
      y: Math.max(-maxY, Math.min(maxY, py)),
    };
  };

  const applyView = (nz: number, np: Pan) => {
    zoomRef.current = nz;
    panRef.current = np;
    setZoom(nz);
    setPan(np);
  };

  const zoomAround = (nz: number, ax?: number, ay?: number) => {
    const z = zoomRef.current;
    const m = metrics();
    const cz = clampZoom(nz, m);
    if (!m || cz === z) {
      applyView(cz, clampPan(panRef.current.x, panRef.current.y, cz, m));
      return;
    }
    const f = cz / z;
    const mx = (ax == null ? m.cx : ax) - m.cx;
    const my = (ay == null ? m.cy : ay) - m.cy;
    const nx = panRef.current.x + (mx - panRef.current.x) * (1 - f);
    const ny = panRef.current.y + (my - panRef.current.y) * (1 - f);
    applyView(cz, clampPan(nx, ny, cz, m));
  };

  const wheelHandlerRef = useRef<((e: WheelEvent) => void) | null>(null);
  wheelHandlerRef.current = (e: WheelEvent) => {
    const tgt = e.target;
    if (tgt instanceof Element && tgt.closest(".map__info, .map__pldwrap, .map__catbar")) return;
    e.preventDefault();
    setAnimate(false);
    window.clearTimeout(wheelEndRef.current);
    wheelEndRef.current = window.setTimeout(() => setAnimate(true), 180);
    zoomAround(zoomRef.current * Math.exp(-e.deltaY * 0.0015), e.clientX, e.clientY);
  };

  useEffect(() => {
    const el = shellRef.current;
    if (!el) return undefined;
    const onWheel = (e: WheelEvent) => wheelHandlerRef.current?.(e);
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  useEffect(() => {
    const el = shellRef.current;
    if (!el || typeof ResizeObserver === "undefined") return undefined;
    const measure = () => {
      const r = el.getBoundingClientRect();
      setBox({ w: r.width, h: r.height, square: Math.min(r.width, r.height) });
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Pointer capture waits for the drag threshold: capturing on pointerdown
  // would retarget the eventual click away from pins, and a drag can start on
  // any pin -- pins select on click, they never swallow a pan.
  const onTilesPointerDown = (e: ReactPointerEvent) => {
    if (e.button != null && e.button !== 0) return;
    dragRef.current = {
      sx: e.clientX,
      sy: e.clientY,
      px: panRef.current.x,
      py: panRef.current.y,
      moved: false,
    };
  };
  const onTilesPointerMove = (e: ReactPointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    const dx = e.clientX - d.sx;
    const dy = e.clientY - d.sy;
    if (!d.moved && Math.hypot(dx, dy) > 3) {
      d.moved = true;
      setGrabbing(true);
      setAnimate(false);
      try {
        e.currentTarget.setPointerCapture(e.pointerId);
      } catch {
      }
    }
    if (d.moved) {
      applyView(zoomRef.current, clampPan(d.px + dx, d.py + dy, zoomRef.current));
    }
  };
  const onTilesPointerUp = (e: ReactPointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    dragRef.current = null;
    try {
      e.currentTarget.releasePointerCapture?.(e.pointerId);
    } catch {
    }
    if (d.moved) {
      draggedRef.current = true;
      setGrabbing(false);
      setAnimate(true);
    }
  };

  const places = useMemo(() => {
    const seen = new Set<string>();
    const merged: PlaceView[] = [];
    for (const p of [...(placesQ.data ?? []), ...(poisQ.data ?? [])]) {
      if (seen.has(p.id)) continue;
      seen.add(p.id);
      merged.push(p);
    }
    return merged;
  }, [placesQ.data, poisQ.data]);

  const detailQ = usePlace(detailOpen ? selectedId : null);
  const selectedFromList = useMemo(
    () => places.find((p) => p.id === selectedId) ?? null,
    [places, selectedId],
  );
  const detailView = detailQ.data ?? selectedFromList;

  const cats = useMemo(() => {
    const live = catsQ.data ?? [];
    return [
      { key: "ALL", label: "ALL", color: "#ffffff" },
      { key: "FAVORITES", label: "FAVORITES", color: "#ff4d6d" },
      ...live.map((c) => ({
        key: c.name.toUpperCase(),
        label: c.label.toUpperCase(),
        color: c.color,
        count: c.count,
      })),
    ];
  }, [catsQ.data]);

  const sidebarCategoryKey = cat !== "ALL" && cat !== "FAVORITES" ? cat.toLowerCase() : null;
  const sidebarLabel = cats.find((c) => c.key === cat);
  const sidebarParams = useMemo(
    () =>
      sidebarCategoryKey
        ? { limit: 50, order_by: sidebarSort, order: "desc" as const, categories: sidebarCategoryKey }
        : { limit: 0 },
    [sidebarCategoryKey, sidebarSort],
  );
  const sidebarQ = usePlaces(sidebarParams, sidebarCategoryKey != null);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return places.filter((p) => {
      if (
        q &&
        !p.title.toLowerCase().includes(q) &&
        !p.coords.toLowerCase().includes(q) &&
        !p.creator.toLowerCase().includes(q)
      ) {
        return false;
      }
      if (cat === "ALL") return true;
      if (cat === "FAVORITES") return p.featured;
      return p.categories.some((c: string) => c.toUpperCase() === cat);
    });
  }, [places, search, cat]);

  const sel =
    filtered.find((p) => p.id === selectedId) ?? selectedFromList ?? null;

  const player = useMemo(() => coordsToPercent(sceneCoords), [sceneCoords]);
  const friendPins = useFriendPins();
  const selectedFriend = useMemo(
    () => friendPins.find((f) => f.address === selFriendAddr) ?? null,
    [friendPins, selFriendAddr],
  );

  const centerOnPercent = (leftPct: number, topPct: number, m?: Metrics | null) => {
    const mm = m || metrics();
    if (!mm) return;
    const cover = coverZoom(mm);
    applyView(
      cover,
      clampPan(
        -(leftPct / 100 - 0.5) * mm.square * cover,
        -(topPct / 100 - 0.5) * mm.square * cover,
        cover,
        mm,
      ),
    );
  };

  const centerOnPlayer = (m?: Metrics | null) => centerOnPercent(player.left, player.top, m);

  const didInitRef = useRef(false);
  useEffect(() => {
    if (!box) return;
    if (!didInitRef.current) {
      didInitRef.current = true;
      centerOnPlayer();
      return;
    }
    const z = clampZoom(zoomRef.current, box);
    applyView(z, clampPan(panRef.current.x, panRef.current.y, z));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [box]);

  const clearSel = useCallback(() => {
    setSelectedId(null);
    setSelFriendAddr(null);
    setDetailOpen(false);
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") clearSel();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [clearSel]);

  const onJumpIn = useCallback(
    (view: (PlaceView & { name?: string }) | null) => {
      if (!view) return;
      teleportTo(view);
      beginJump(view.name || "destination");
    },
    [beginJump],
  );

  const jumpToFriend = useCallback(
    (f: FriendPin) => {
      sendBridge("Teleport", {
        x: f.x * PARCEL_SIZE + PARCEL_SIZE / 2,
        z: f.y * PARCEL_SIZE + PARCEL_SIZE / 2,
      });
      beginJump(f.name);
    },
    [beginJump],
  );

  // Worlds are a full realm change (leaves the current server), so confirm before jumping --
  // unlike a Genesis City parcel teleport, which stays on the same realm.
  const requestJumpIn = useCallback(
    (view: (PlaceView & { name?: string }) | null) => {
      if (!view) return;
      if (view.world && view.worldName) {
        setConfirmWorld({ realm: view.worldName, title: view.title });
        return;
      }
      onJumpIn(view);
    },
    [onJumpIn],
  );

  const confirmVisitWorld = useCallback(() => {
    if (!confirmWorld) return;
    sendBridge("ChangeRealm", { realm: confirmWorld.realm });
    setConfirmWorld(null);
    setDetailOpen(false);
    beginJump(confirmWorld.title || confirmWorld.realm);
  }, [confirmWorld, beginJump]);

  const pickWorldHit = useCallback((w: PlaceView) => {
    setConfirmWorld({ realm: w.worldName || w.title, title: w.title });
    setSearch("");
  }, []);

  const pickPlaceHit = useCallback((p: PlaceView) => {
    centerOnPercent(p.left, p.top);
    setSelectedId(p.id);
    setDetailOpen(false);
    setSearch("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onDetailClick = useCallback(
    (e: ReactMouseEvent) => {
      const t = e.target instanceof Element ? e.target : null;
      if (t?.closest(".pld__jump") || t?.closest(".pld__nav")) {
        requestJumpIn(detailView ?? sel);
        setDetailOpen(false);
        return;
      }
      if (t?.closest(".pld__close")) {
        setDetailOpen(false);
        return;
      }
      if (!t?.closest(".pld")) setDetailOpen(false);
    },
    [detailView, sel, requestJumpIn],
  );

  const loading = placesQ.isLoading;
  const error = placesQ.isError;
  const empty = !loading && !error && filtered.length === 0;

  return (
    <div className="map__shell" ref={shellRef}>
      {jumping && (
        <JumpLoading
          name={jumping}
          stalled={stalled}
          onCancel={cancelJump}
          onEnterAnyway={confirmJump}
        />
      )}
      <div
        className={"map__tiles" + (grabbing ? " is-grabbing" : "")}
        onClick={() => {
          if (draggedRef.current) {
            draggedRef.current = false;
            return;
          }
          clearSel();
        }}
        onPointerDown={onTilesPointerDown}
        onPointerMove={onTilesPointerMove}
        onPointerUp={onTilesPointerUp}
        onPointerCancel={onTilesPointerUp}
        style={{
          ...(box ? { width: box.square, height: box.square } : null),
          transform: `translate(-50%, -50%) translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          transformOrigin: "center center",
          transition: animate ? "transform 0.15s ease" : "none",
        }}
      >
        <div className="map__grid" />
        <div className="map__roads" />
        {slippyTilesFor(zoom, box && { ...box, panX: pan.x, panY: pan.y }).map((t) => (
          <img
            key={t.key}
            className="map__satellite"
            src={t.src}
            alt=""
            draggable={false}
            loading="lazy"
            style={{
              position: "absolute",
              left: t.left + "%",
              top: t.top + "%",
              width: `calc(${t.size}% + 0.5px)`,
              height: `calc(${t.size}% + 0.5px)`,
              imageRendering: "pixelated",
              pointerEvents: "none",
            }}
          />
        ))}

        <div
          className="map__player"
          style={{ left: player.left + "%", top: player.top + "%" }}
          aria-label="Your location"
        >
          <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
            <path d="M12 3 20 21l-8-5-8 5 8-18z" fill="#fff" />
          </svg>
        </div>

        {filtered.map((p) => (
          <button
            key={p.id}
            className={
              "map__pin map__pin--" +
              p.kind +
              (p.id === selectedId ? " is-selected" : "")
            }
            style={{ left: p.left + "%", top: p.top + "%" }}
            onClick={(e) => {
              e.stopPropagation();
              setSelectedId(p.id);
              setSelFriendAddr(null);
              setDetailOpen(false);
            }}
            aria-label={p.title}
            title={`${p.title} (${p.coords})`}
          >
            <PinIcon kind={p.kind} />
          </button>
        ))}

        {friendPins.map((f) => (
          <button
            key={f.address}
            className={
              "map__pin map__pin--friend" +
              (f.address === selFriendAddr ? " is-selected" : "")
            }
            style={{ left: f.left + "%", top: f.top + "%" }}
            onClick={(e) => {
              e.stopPropagation();
              setSelFriendAddr(f.address);
              setSelectedId(null);
              setDetailOpen(false);
            }}
            aria-label={f.name}
            title={`${f.name} (${f.coords})`}
          >
            <PinIcon kind="friend" picture={f.picture} />
            <span className="map__pinfriendname">{f.name}</span>
          </button>
        ))}
      </div>

      <div className="map__catbar">
        <div className="map__cats" role="tablist" aria-label="Place categories">
          {cats.map((c) => (
            <button
              key={c.key}
              type="button"
              role="tab"
              aria-selected={c.key === cat}
              className={"map__catpill" + (c.key === cat ? " is-on" : "")}
              onClick={() => {
                setCat(c.key);
                setSelectedId(null);
              }}
            >
              {c.key === "ALL" ? (
                <span className="map__catglyph map__catglyph--all" aria-hidden="true">
                  &#x25A6;
                </span>
              ) : c.key === "FAVORITES" ? (
                <span
                  className="map__catglyph map__catglyph--heart"
                  aria-hidden="true"
                  style={{ color: c.color }}
                >
                  &#x2665;
                </span>
              ) : (
                <span
                  className="map__catdot"
                  aria-hidden="true"
                  style={{ background: c.color }}
                />
              )}
              {c.label}
            </button>
          ))}
        </div>
        <div className="map__search">
          <SearchField placeholder="Search places & worlds" value={search} onChange={setSearch} />
          {search.trim().length >= 2 && (searchWorldHits.length > 0 || searchPlaceHits.length > 0) && (
            <div className="map__searchresults">
              {searchWorldHits.map((w) => (
                <button key={w.id} type="button" className="map__searchresult" onClick={() => pickWorldHit(w)}>
                  <span className="map__searchresultworldicon" aria-hidden="true">&#x1F310;</span>
                  <span className="map__searchresultbody">
                    <span className="map__searchresulttitle">{w.title || w.worldName}</span>
                    <span className="map__searchresultsub">{w.worldName}</span>
                  </span>
                  <span className="map__searchresulttag">WORLD</span>
                </button>
              ))}
              {searchPlaceHits.map((p) => (
                <button key={p.id} type="button" className="map__searchresult" onClick={() => pickPlaceHit(p)}>
                  <span
                    className="map__searchresultimg"
                    style={{ "--hue": p.hue, ...(p.image ? { backgroundImage: `url("${p.image}")` } : null) } as CSSProperties}
                  />
                  <span className="map__searchresultbody">
                    <span className="map__searchresulttitle">{p.title}</span>
                    <span className="map__searchresultsub">{p.coords}</span>
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="map__zoom">
        <div className="map__zgroup">
          <button
            className="map__zbtn"
            aria-label="Zoom in"
            onClick={() => zoomAround(zoomRef.current + ZOOM_STEP)}
          >
            +
          </button>
          <button
            className="map__zbtn"
            aria-label="Zoom out"
            onClick={() => zoomAround(zoomRef.current - ZOOM_STEP)}
          >
            &#x2212;
          </button>
        </div>
        <button
          className="map__zbtn map__locate"
          aria-label="Recenter"
          onClick={() => {
            clearSel();
            centerOnPlayer();
          }}
        >
          &#x2295;
        </button>
      </div>

      <div className="map__minimap" aria-hidden="true">
        <div className="map__minigrid" />
        <img className="map__miniimg" src={MINIMAP_SRC} alt="" draggable={false} />
        {friendPins.map((f) => (
          <div
            key={f.address}
            className="map__minifriend"
            style={{ left: f.left + "%", top: f.top + "%" }}
          />
        ))}
        <div
          className="map__minihere"
          style={{ left: player.left + "%", top: player.top + "%" }}
        />
      </div>

      <div className="map__credit">
        {loading
          ? "Loading places\u{2026}"
          : `${filtered.length} place${filtered.length === 1 ? "" : "s"} \u{B7} live from ${new URL(serviceBase("places")).host}`}
      </div>

      {loading && (
        <div className="map__info" role="status">
          <div className="map__infobody">
            <div className="map__infoname">Loading places&#x2026;</div>
            <div className="map__infocreator">fetching live data from catalyst</div>
          </div>
        </div>
      )}

      {error && (
        <div className="map__info" role="alert">
          <div className="map__infobody">
            <div className="map__infoname">Couldn&apos;t load places</div>
            <div className="map__infocreator">
              {placesQ.error?.message || "Network error"}
            </div>
            <div className="map__infoactions">
              <button className="map__jump" onClick={() => placesQ.refetch()}>
                retry
              </button>
            </div>
          </div>
        </div>
      )}

      {empty && !sel && !selectedFriend && (
        <div className="map__info">
          <div className="map__infobody">
            <div className="map__infoname">No places match</div>
            <div className="map__infocreator">
              try a different category or search term
            </div>
          </div>
        </div>
      )}

      {selectedFriend && (
        <div className="map__info">
          <div
            className="map__infothumb"
            style={cssVars({
              "--hue": 210,
              ...(safeCssUrl(selectedFriend.picture)
                ? {
                    background: `#0e0e12 center/cover no-repeat ${safeCssUrl(selectedFriend.picture)}`,
                  }
                : null),
            })}
          >
            <button
              className="map__infoclose"
              onClick={clearSel}
              aria-label="Close"
            >
              &#xD7;
            </button>
          </div>
          <div className="map__infobody">
            <div className="map__infoname">{selectedFriend.name}</div>
            <div className="map__infocreator">friend &#xB7; online in world</div>
            <div className="map__inforow">
              <span className="map__infostat">
                <b>{selectedFriend.coords}</b>
                <span>LOCATION</span>
              </span>
            </div>
            <div className="map__infoactions">
              <button
                className="map__jump"
                onClick={() => jumpToFriend(selectedFriend)}
              >
                jump in
              </button>
            </div>
          </div>
        </div>
      )}

      {sel && !detailOpen && (
        <div className="map__info">
          <div
            className="map__infothumb"
            style={cssVars({
              "--hue": sel.hue,
              ...(sel.image
                ? {
                    background: `#0e0e12 center/cover no-repeat url("${sel.image}")`,
                  }
                : null),
            })}
          >
            {sel.live && <span className="map__infolive">&#x25CF; {sel.players} LIVE</span>}
            <button
              className="map__infoclose"
              onClick={clearSel}
              aria-label="Close"
            >
              &#xD7;
            </button>
          </div>
          <div className="map__infobody">
            <div className="map__infoname">{sel.title}</div>
            <div className="map__infocreator">
              created by <b>{sel.creator}</b>
            </div>
            <div className="map__inforow">
              <span className="map__infostat">
                <b>{sel.coords}</b>
                <span>LOCATION</span>
              </span>
              <span className="map__infostat">
                <b>{sel.rating}%</b>
                <span>RATING</span>
              </span>
              <span className="map__infostat">
                <b>{sel.players ?? "\u{2014}"}</b>
                <span>VISITORS</span>
              </span>
            </div>
            <div className="map__infoactions">
              <button className="map__jump" onClick={() => requestJumpIn(sel)}>
                jump in
              </button>
              <button className="map__nav" onClick={() => setDetailOpen(true)}>
                details
              </button>
            </div>
          </div>
        </div>
      )}

      {detailOpen && sel && (
        <div className="map__pldwrap" onClick={onDetailClick}>
          <PlaceDetail
            place={toPlaceDetail(detailView ?? sel) ?? undefined}
            notFound={detailQ.isError && !selectedFromList}
          />
        </div>
      )}

      {sidebarCategoryKey && (
        <CategorySidebar
          label={sidebarLabel?.label ?? sidebarCategoryKey}
          color={sidebarLabel?.color ?? "#ffffff"}
          sort={sidebarSort}
          setSort={setSidebarSort}
          places={sidebarQ.data ?? []}
          loading={sidebarQ.isLoading}
          onClose={() => setCat("ALL")}
          onSelect={(p) => {
            setSelectedId(p.id);
            setDetailOpen(false);
          }}
        />
      )}

      {confirmWorld && (
        <WorldVisitModal
          worldName={confirmWorld.realm}
          title={confirmWorld.title}
          onCancel={() => setConfirmWorld(null)}
          onConfirm={confirmVisitWorld}
        />
      )}
    </div>
  );
}
