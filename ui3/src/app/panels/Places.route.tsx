import type { QueryClient } from "@tanstack/react-query";
import { useCallback, useMemo, useState } from "react";
import type { CSSProperties, KeyboardEvent, MouseEvent } from "react";
import { useNavigate } from "react-router";

import Places from "../../explorer/pages/Places";
import PlaceDetail from "../../explorer/pages/PlaceDetail";
import JumpLoading, { useJump } from "../../explorer/components/JumpLoading";
import WorldVisitModal from "../../components/WorldVisitModal";
import { usePlaces } from "../../data/hooks/usePlaces";
import { toPlaceDetail } from "../../data/catalyst/places";
import { fetchPlaces } from "../../data/catalyst/placesSchema";
import type { PlaceView } from "../../data/catalyst/places";
import { sendBridge, getDeployIdentity } from "../../overlay/bridge";
import { qk, STALE } from "../../data/queryKeys";
import { RecentPlacesSchema } from "../../data/persisted-schemas";
import { check } from "../../validate";

const LIST_PARAMS = { limit: 60 };
const PARCEL_SIZE = 16;
const RECENT_KEY = "dcl.recentPlaces";

const CONTENTS_STYLE: CSSProperties = { display: "contents" };

function getRecent(): PlaceView[] {
  if (typeof localStorage === "undefined") return [];
  // The try covers the read and the parse, the two things that throw on their
  // own. `check` throws in dev on a drifted entry, and a catch wide enough to
  // cover it would turn that into a silently empty Recent tab.
  let parsed: unknown;
  try {
    parsed = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const recents = check(RecentPlacesSchema, parsed, "persisted/recent-places");
  // A JSON round trip drops undefined values, so `image` comes back absent
  // rather than present-and-undefined. Restoring it is what makes these
  // `PlaceView`s again.
  return recents.map((p) => ({ ...p, image: p.image }));
}

function pushRecent(p: PlaceView): void {
  if (typeof localStorage === "undefined") return;
  const cur = getRecent().filter((x) => x.id !== p.id);
  cur.unshift(p);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(cur.slice(0, 24)));
  } catch {
  }
}

export function prefetch(queryClient: QueryClient) {
  try {
    queryClient.prefetchQuery({
      queryKey: qk.places(LIST_PARAMS),
      queryFn: ({ signal }) => fetchPlaces(LIST_PARAMS, { signal }),
      staleTime: STALE.places,
    });
  } catch {
  }
}

export default function PlacesPanel() {
  const navigate = useNavigate();
  const { jumping, stalled, beginJump, cancelJump, confirmJump } = useJump(() => navigate("/"));
  const [section, setSection] = useState("explore");
  const [sort, setSort] = useState("most_active");
  const addr = getDeployIdentity()?.signerAddress ?? null;
  const params = useMemo(() => {
    const base = { limit: 60, order_by: sort, order: "desc" as const };
    if (section === "favorites") return { ...base, only_favorites: true };
    if (section === "myplaces")
      return { ...base, owner: addr ?? "0x0000000000000000000000000000000000000000" };
    return base;
  }, [section, sort, addr]);
  const q = usePlaces(params);
  const [selected, setSelected] = useState<(PlaceView & { name?: string }) | null>(null);
  const [confirmWorld, setConfirmWorld] = useState<{ realm: string; title?: string } | null>(null);
  const places: PlaceView[] = useMemo(
    () => (section === "recent" ? getRecent() : Array.isArray(q.data) ? q.data : []),
    [section, q.data],
  );
  const cards = useMemo(
    () =>
      places.map((p) => ({
        ...p,
        live: p.players != null && p.players > 0 ? p.players : undefined,
        to: "Explorer/Pages/PlaceDetail",
      })),
    [places],
  );

  const findByCardId = useCallback(
    (cardId: string | null) => places.find((p) => (p.id ?? p.title) === cardId) ?? null,
    [places],
  );

  const onGridClick = useCallback(
    (e: MouseEvent<HTMLDivElement>) => {
      const target = e.target;
      const card = target instanceof Element ? target.closest<HTMLElement>("[data-place-id]") : null;
      const place = card ? findByCardId(card.dataset.placeId ?? null) : null;
      if (!place) return;
      setSelected(place);
    },
    [findByCardId],
  );

  const onJumpIn = useCallback(() => {
    const place = selected;
    setSelected(null);
    if (!place) return;
    pushRecent(place);
    let jumped = false;
    if (place.world) {
      if (place.worldName) {
        sendBridge("ChangeRealm", { realm: place.worldName });
        jumped = true;
      }
    } else {
      const px = Number(place.x);
      const py = Number(place.y);
      if (Number.isFinite(px) && Number.isFinite(py)) {
        sendBridge("Teleport", {
          x: px * PARCEL_SIZE + PARCEL_SIZE / 2,
          z: py * PARCEL_SIZE + PARCEL_SIZE / 2,
        });
        jumped = true;
      }
    }
    if (!jumped) return;
    beginJump(place.title || place.name || "destination");
  }, [selected, beginJump]);

  // A world is a full realm change (leaves the current server), so confirm before
  // ChangeRealm -- a Genesis City parcel teleport stays on the same realm and needs none.
  const requestJumpIn = useCallback(() => {
    if (selected?.world && selected.worldName) {
      setConfirmWorld({ realm: selected.worldName, title: selected.title });
      return;
    }
    onJumpIn();
  }, [selected, onJumpIn]);

  const confirmVisitWorld = useCallback(() => {
    if (!confirmWorld) return;
    const place = selected;
    setConfirmWorld(null);
    setSelected(null);
    if (place) pushRecent(place);
    sendBridge("ChangeRealm", { realm: confirmWorld.realm });
    beginJump(confirmWorld.title || confirmWorld.realm);
  }, [confirmWorld, selected, beginJump]);

  return (
    <div
      style={CONTENTS_STYLE}
      onClick={onGridClick}
      onKeyDown={(e: KeyboardEvent<HTMLDivElement>) => {
        if (e.key !== "Enter" && e.key !== " ") return;
        const target = e.target;
        const card = target instanceof Element ? target.closest<HTMLElement>("[data-place-id]") : null;
        if (!card) return;
        const place = findByCardId(card.dataset.placeId ?? null);
        if (place) {
          e.preventDefault();
          setSelected(place);
        }
      }}
    >
      <Places
        places={cards}
        loading={section === "recent" ? false : q.isLoading}
        error={section === "recent" ? false : q.isError}
        section={section}
        onSectionChange={setSection}
        sort={sort}
        onSortChange={setSort}
      />
      {selected ? (
        <PlaceDetail
          place={toPlaceDetail(selected) ?? undefined}
          onClose={() => setSelected(null)}
          onJumpIn={requestJumpIn}
        />
      ) : null}
      {confirmWorld && (
        <WorldVisitModal
          worldName={confirmWorld.realm}
          title={confirmWorld.title}
          onCancel={() => setConfirmWorld(null)}
          onConfirm={confirmVisitWorld}
        />
      )}
      {jumping && (
        <JumpLoading
          name={jumping}
          stalled={stalled}
          onCancel={cancelJump}
          onEnterAnyway={confirmJump}
        />
      )}
    </div>
  );
}
