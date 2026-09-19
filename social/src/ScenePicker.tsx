import { Picture } from "./Picture";
import { useEffect, useRef, useState } from "react";
import { api, type Scene } from "./api";
import { destinationKey, destinationLabel, worldName } from "./destinations";
type Place = {
  world_name?: string;
  id: string;
  title: string;
  base_position: string;
  image?: string;
};
export function ScenePicker({
  onSelect,
  onClose,
  purpose = "share",
}: {
  purpose?: "share" | "event";
  onSelect: (scene: Scene) => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [selected, setSelected] = useState<{
    scene: Scene;
    title: string;
    image?: string;
  } | null>(null);
  const [recent] = useState<Scene[]>(() => {
    try {
      return JSON.parse(
        localStorage.getItem("dcl.social.recent-places") || "[]",
      )
        .filter(
          (s: Scene) =>
            (!s.world || !!worldName(s.world)) &&
            Number.isInteger(s.x) &&
            Number.isInteger(s.y) &&
            Math.abs(s.x) <= 150 &&
            Math.abs(s.y) <= 150,
        )
        .slice(0, 4);
    } catch {
      return [];
    }
  });
  const [worlds, setWorlds] = useState(false);
  const [search, setSearch] = useState("");
  const [places, setPlaces] = useState<Place[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  useEffect(() => {
    const abort = new AbortController();
    setLoading(true);
    setError("");
    setPlaces([]);
    const timer = setTimeout(() => {
      api<{ data: Place[] }>(
        `/${worlds ? "worlds" : "places"}?search=${encodeURIComponent(search)}`,
        {
          signal: abort.signal,
        },
      )
        .then((p) => {
          if (abort.signal.aborted) return;
          setPlaces(Array.isArray(p.data) ? p.data : []);
          setError("");
        })
        .catch((e) => {
          if (e.name !== "AbortError")
            setError(
              "Scene search is unavailable. You can still enter coordinates.",
            );
        })
        .finally(() => {
          if (!abort.signal.aborted) setLoading(false);
        });
    }, 250);
    return () => {
      clearTimeout(timer);
      abort.abort();
    };
  }, [search, worlds]);
  const parse = (value: unknown): Scene | null => {
    if (typeof value !== "string") return null;
    const world = worldName(value);
    if (world) return { x: 0, y: 0, world };
    try {
      const url = new URL(value);
      value = url.searchParams.get("position") || value;
    } catch {
      /* Coordinates and names are also accepted. */
    }
    const m = (value as string).trim().match(/^(-?\d{1,3})\s*,\s*(-?\d{1,3})$/);
    if (!m || Math.abs(Number(m[1])) > 150 || Math.abs(Number(m[2])) > 150)
      return null;
    return { x: Number(m[1]), y: Number(m[2]) };
  };
  const coordinates = parse(search);
  return (
    <dialog
      ref={dialog}
      className="dialog scene-dialog"
      aria-labelledby="scene-title"
      onCancel={onClose}
      onClick={(e) => {
        if (e.target === dialog.current) onClose();
      }}
    >
      <div>
        <header>
          <h2 id="scene-title">{purpose === "event" ? "Choose a location" : "Share a scene"}</h2>
          <button onClick={onClose} aria-label="Close scene picker">
            &#xd7;
          </button>
        </header>
        <div className="discovery-actions">
          <button
            className={!worlds ? "primary" : "outline-button"}
            onClick={() => {
              setWorlds(false);
              setSelected(null);
            }}
          >
            Genesis City
          </button>
          <button
            className={worlds ? "primary" : "outline-button"}
            onClick={() => {
              setWorlds(true);
              setSelected(null);
            }}
          >
            Worlds
          </button>
        </div>
        <input
          autoFocus
          aria-label="Find a scene"
          placeholder="Search places, coordinates or a link"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {!search && recent.length > 0 && (
          <div className="recent-places">
            <small>Recently shared</small>
            {recent.map((s) => (
              <button
                key={destinationKey(s)}
                className="outline-button"
                onClick={() =>
                  setSelected({ scene: s, title: destinationLabel(s) })
                }
              >
                {destinationLabel(s)}
              </button>
            ))}
          </div>
        )}
        {selected && (
          <div className="selected-place">
            <Picture src={selected.image} />
            <div>
              <strong>{selected.title}</strong>
              <small>{destinationLabel(selected.scene)}</small>
            </div>
            <button
              className="primary"
              onClick={() => {
                try {
                  localStorage.setItem(
                    "dcl.social.recent-places",
                    JSON.stringify(
                      [
                        selected.scene,
                        ...recent.filter(
                          (s) =>
                            destinationKey(s) !==
                            destinationKey(selected.scene),
                        ),
                      ].slice(0, 4),
                    ),
                  );
                } catch {}
                onSelect(selected.scene);
              }}
            >
              {purpose === "event" ? "Use this location" : "Share scene"}
            </button>
          </div>
        )}
        {coordinates && (
          <button
            className="place-result"
            onClick={() =>
              setSelected({
                scene: coordinates,
                title: destinationLabel(coordinates),
              })
            }
          >
            <span>&#x25c7;</span>
            <div>
              {coordinates.world
                ? destinationLabel(coordinates)
                : `Parcel ${destinationLabel(coordinates)}`}
              <small>Share this location</small>
            </div>
            <b>&#xff0b;</b>
          </button>
        )}
        <div className="place-results">
          {places
            .filter((p) => parse(p.world_name || p.base_position))
            .map((p) => (
              <button
                className="place-result"
                key={p.id}
                onClick={() =>
                  setSelected({
                    scene: parse(p.world_name || p.base_position)!,
                    title: p.title,
                    image: p.image,
                  })
                }
              >
                <Picture src={p.image} />
                <div>
                  {p.title}
                  <small>{p.world_name || p.base_position}</small>
                </div>
                <b>&#xff0b;</b>
              </button>
            ))}
        </div>
        {error && <p className="muted">{error}</p>}
        {loading && (
          <p className="muted" role="status">
            Finding scenes&#x2026;
          </p>
        )}
        {!loading &&
          !error &&
          !places.some((p) => parse(p.world_name || p.base_position)) &&
          !coordinates && (
            <p className="muted">
              No scenes found. Try a name or parcel coordinates.
            </p>
          )}
      </div>
    </dialog>
  );
}
