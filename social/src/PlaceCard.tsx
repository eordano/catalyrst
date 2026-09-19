import { useEffect, useState } from "react";
import { api, type Scene } from "./api";
import {
  destinationKey,
  destinationLabel,
  destinationUrl,
} from "./destinations";
import { Picture } from "./Picture";
type Place = { title?: string; image?: string };
const cache = new Map<string, Promise<Place | null>>();
export function PlaceCard({ scene }: { scene: Scene }) {
  const key = destinationKey(scene),
    label = destinationLabel(scene);
  const [place, setPlace] = useState<Place | null>(null);
  useEffect(() => {
    let active = true;
    setPlace(null);
    if (!cache.has(key)) {
      if (cache.size > 200) cache.clear();
      cache.set(
        key,
        api<{ data: Place[] }>(
          scene.world
            ? `/worlds?name=${encodeURIComponent(scene.world)}`
            : `/places?position=${key}`,
        )
          .then((r) => r.data[0] || null)
          .catch(() => null),
      );
    }
    cache.get(key)!.then((p) => {
      if (active) setPlace(p);
    });
    return () => {
      active = false;
    };
  }, [key, scene.world]);
  return (
    <a
      className="place-card"
      aria-label={`Explore together at ${label}`}
      href={destinationUrl(scene)}
      target="_blank"
      rel="noreferrer"
    >
      <div className="place-card-art">
        <Picture src={place?.image} fallback={scene.world ? "\u25c8" : "\u25c7"} />
      </div>
      <div className="place-card-info">
        <div>
          <strong>
            {place?.title ||
              (scene.world ? "Explore this world" : "Explore together")}
          </strong>
          <small>{label}</small>
        </div>
        <b>Jump in &#x2197;</b>
      </div>
    </a>
  );
}
