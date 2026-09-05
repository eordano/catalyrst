import { useMemo, useState } from "react";

import SearchField from "../../atoms/SearchField";
import Button from "../../atoms/Button";
import Dropdown from "../../components/Dropdown";
import EmptyState from "../../components/EmptyState";
import Spinner from "../../atoms/Spinner";
import PlaceCard from "../../components/PlaceCard";
import { usePlaces, useWorlds } from "../../data/hooks/usePlaces";
import { isEnsQuery } from "../../data/hooks/usePlaceSearch";
import type { PlaceView } from "../../data/catalyst/places";
import "../pages/places.css";
import "./placespicker.css";

export type PickedDestination =
  | { kind: "world"; realm: string }
  | { kind: "parcel"; x: number; y: number }
  | null;

const SORT_OPTIONS: { label: string; value: string }[] = [
  { label: "Most active", value: "most_active" },
  { label: "Best rated", value: "like_score" },
  { label: "Newest", value: "created_at" },
];
const DEFAULT_SORT = { label: "Most active", value: "most_active" };

function destinationFor(p: PlaceView): PickedDestination {
  if (p.world) {
    return p.worldName ? { kind: "world", realm: p.worldName } : null;
  }
  if (!Number.isFinite(p.x) || !Number.isFinite(p.y)) return null;
  return { kind: "parcel", x: p.x, y: p.y };
}

type PlacesPickerProps = { onPick: (dest: PickedDestination) => void };

export default function PlacesPicker({ onPick }: PlacesPickerProps) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState("most_active");
  const params = useMemo(
    () => ({ limit: 48, order_by: sort, order: "desc" as const, search: query.trim() || undefined }),
    [sort, query],
  );
  const q = usePlaces(params);
  const trimmed = query.trim();
  // A search should also surface Worlds (e.g. boedo.dcl.eth) -- /api/places only covers
  // Genesis City, so merge in a /api/worlds lookup while the user is searching.
  const worldParams = useMemo(
    () => (isEnsQuery(trimmed) ? { limit: 24, names: trimmed } : { limit: 24, search: trimmed }),
    [trimmed],
  );
  const worldsQ = useWorlds(worldParams, trimmed.length >= 2);
  const places = useMemo(() => {
    const list = q.data ?? [];
    if (trimmed.length < 2) return list;
    const seen = new Set(list.map((p) => p.id));
    return [...list, ...(worldsQ.data ?? []).filter((w) => !seen.has(w.id))];
  }, [q.data, worldsQ.data, trimmed]);

  const pick = (p: PlaceView) => {
    const dest = destinationFor(p);
    if (dest) onPick(dest);
  };

  return (
    <div className="pkr">
      <div className="pkr__scroll">
        <div className="pkr__inner">
          <div className="pkr__head">
            <div>
              <p className="pkr__kicker">Welcome back</p>
              <h1 className="pkr__title">Where do you want to go?</h1>
            </div>
            <Button variant="secondary" size="md" className="pkr__skip" onClick={() => onPick(null)}>
              Skip to Genesis Plaza
            </Button>
          </div>

          <div className="pkr__controls">
            <div className="pkr__search">
              <SearchField placeholder="Search places & worlds" value={query} onChange={setQuery} />
            </div>
            <div className="pkr__sort">
              <Dropdown
                ariaLabel="Sort places"
                options={SORT_OPTIONS.map((o) => o.label)}
                value={SORT_OPTIONS.find((o) => o.value === sort)?.label ?? DEFAULT_SORT.label}
                onChange={(label) => setSort(SORT_OPTIONS.find((o) => o.label === label)?.value ?? DEFAULT_SORT.value)}
              />
            </div>
          </div>

          {q.isLoading ? (
            <div className="pkr__center"><Spinner size={34} /></div>
          ) : q.isError ? (
            <EmptyState variant="inline" tone="error" title="Couldn't load places" subtitle="Check your connection and try again." />
          ) : places.length === 0 ? (
            <EmptyState variant="inline" title="No results" subtitle="Nothing matched your search." />
          ) : (
            <div className="pl__grid">
              {places.map((p, i) => (
                <div className="pl__cardwrap" key={p.id} onClick={() => pick(p)}>
                  <PlaceCard
                    title={p.title}
                    image={p.image}
                    players={p.players}
                    rating={p.rating}
                    coords={p.coords}
                    live={p.players != null && p.players > 0 ? p.players : undefined}
                    featured={p.featured}
                    creator={p.creator}
                    hue={(i * 47) % 360}
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
