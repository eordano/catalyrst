import FreshnessNotice from "../../components/FreshnessNotice";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import Button from "../../atoms/Button";
import { Avatar } from "../../atoms/primitives";
import { Close, Search } from "../../atoms/icons";
import { SidebarGlyph } from "../frames/Sidebar";
import { usePlaces, useWorlds } from "../../data/hooks/usePlaces";
import { useEvents } from "../../data/hooks/useEvents";
import { useFriendPins } from "../../data/hooks/useFriendPins";
import { useNotifications } from "../../data/hooks/useNotifications";
import { useOwnedWearables } from "../../data/hooks/useOwnedItems";
import { PLAY_EVENTS_PARAMS, PLAY_UPCOMING_PARAMS, PLAY_LOBBY_PLACES_PARAMS } from "../../data/screens/play";
import { hexToColor3, color3ToHex } from "../../data/catalyst/backpack";
import { catalystBase, sendSignedJSON } from "../../data/catalyst/client";
import { useQuery } from "@tanstack/react-query";
import { LobbyCarousel, LobbyIcon, PlaceCard, EventCard } from "./lobby/LobbyCards";
import type { PlaceView } from "../../data/catalyst/places";
import { getRecent, pushRecent } from "../../data/recentPlaces";
import { useBridgeState, sendBridge } from "../../overlay/bridge";
import WearablePreview from "../../wearable-preview/WearablePreview";
import type { AvatarStatus } from "../../wearable-preview/avatar";
import JumpLoading, { useJump } from "../components/JumpLoading";
import { parcelDestination, placeDestination, type LobbyDestination } from "./lobbyDestinations";
import LobbyHeaderMenu from "./lobby/LobbyHeaderMenu";
import LobbySceneCard from "./lobby/LobbySceneCard";
import logo from "./lobby/assets/decentraland-logo.svg?url";
import { useWorldEntry } from "../../app/WorldEntry";
import "./lobbyhome.css";
import "./lobby/lobby-experience.css";

function LoadState({ pending, error, empty, retry }: { pending: boolean; error: boolean; empty: string; retry: () => unknown }) {
  if (pending) return <p className="lh__note" role="status" aria-busy="true">Loading&#x2026;</p>;
  if (error) return <p className="lh__note" role="alert">Couldn&#x2019;t load this section. <Button variant="ghost" size="sm" onClick={retry}>Retry</Button></p>;
  return <p className="lh__note">{empty}</p>;
}

export default function LobbyHome({ onEnterWorld, onSignOut, onReady }: { onEnterWorld: () => void; onSignOut?: () => void; onReady?: () => void }) {
  const navigate = useNavigate();
  const entry = useWorldEntry();
  const identity = useBridgeState((s) => s.identity);
  const scene = useBridgeState(s => s.scene);
  const parcel = useBridgeState(s => s.playerPosition?.parcel);
  const resuming = entry?.pending === false;
  const destination = entry?.destination;
  const [launch] = useState(() => new URLSearchParams(window.location.search));
  const target = destination?.kind === "world" ? { realm: destination.realm, coords: destination.parcel?.join(",") }
    : destination?.kind === "parcel" ? { coords: `${destination.x},${destination.y}` } : { realm: launch.get("realm") ?? undefined, coords: launch.get("position") ?? undefined };
  const linkedPreview = launch.has("preview") || launch.has("systemScene");
  const avatarBase = useBridgeState(s => s.avatarBase);
  const loadout = useBridgeState((s) => s.avatarLoadout);
  const friendState = useBridgeState((s) => s.friends);
  const pins = useFriendPins();
  const { unread } = useNotifications();
  const owned = useOwnedWearables(identity.address);
  const equipped = owned.data?.equipped;
  const avatarPreview = useBridgeState(s => s.avatarPreview);
  const credits = useQuery({
    queryKey: ["lobby-credits", identity.address],
    enabled: !identity.isGuest && !!identity.address,
    retry: false,
    staleTime: 60000,
    queryFn: async () => {
      const path = "/credits/wallet/" + encodeURIComponent(identity.address!) + "/balance";
      const result = await sendSignedJSON<{ available: string }>(path, { method: "GET" });
      return result && /^\d+(\.\d+)?$/.test(result.available) ? result.available : null;
    },
  });
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus({ preventScroll: true }); }, []);
  const [searchOpen, setSearchOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [query, setQuery] = useState("");
  const [recents] = useState(getRecent);
  const [avatarStatus, setAvatarStatus] = useState<AvatarStatus>("loading");
  useEffect(() => { if (avatarStatus === "ready") onReady?.(); }, [avatarStatus, onReady]);
  const [avatarAttempt, setAvatarAttempt] = useState(0);
  const [headerMenu, setHeaderMenu] = useState<"account" | "notifications" | null>(null);
  const look = {
    name: identity.name || "",
    bodyShape: loadout?.bodyShape || equipped?.bodyShape || "urn:decentraland:off-chain:base-avatars:BaseMale",
    wearables: loadout?.wearables ?? equipped?.wearables ?? [],
    emotes: loadout?.emotes ?? equipped?.emotes ?? [],
    skinColor: avatarBase?.skinColor ? (color3ToHex(avatarBase.skinColor) || "#c98c63") : equipped?.skinColor || "#c98c63",
    hairColor: avatarBase?.hairColor ? (color3ToHex(avatarBase.hairColor) || "#5c3824") : equipped?.hairColor || "#5c3824",
    eyeColor: avatarBase?.eyesColor ? (color3ToHex(avatarBase.eyesColor) || "#3a6ea5") : equipped?.eyeColor || "#3a6ea5",
  };
  function editAvatar() { navigate("/backpack"); }
  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(search.trim()), 250);
    return () => window.clearTimeout(timer);
  }, [search]);
  const featured = usePlaces(PLAY_LOBBY_PLACES_PARAMS);
  const results = usePlaces({ search: query, limit: 24 }, !!query);
  const worlds = useWorlds({ search: query, limit: 24 }, !!query);
  const events = useEvents(PLAY_EVENTS_PARAMS);
  const upcoming = useEvents(PLAY_UPCOMING_PARAMS);
  const upcomingEvents = upcoming.data?.data.filter(event => !event.live) ?? [];
  const liveEvents = events.data?.data.filter((event) => event.live) ?? [];
  const featuredPlaces = featured.data ?? [];
  const online = friendState.friends.filter((friend) => friend.status === "online" && !friendState.blocked.some((address) => address.toLowerCase() === friend.address.toLowerCase()));
  const hero = featuredPlaces.find(place => /genesis plaza/i.test(place.title)) ?? featuredPlaces[0];
  const jump = useJump(onEnterWorld);
  const placeKey = (place: PlaceView) => place.world ? `world:${place.worldName}` : `land:${place.coords}`;
  const recentPlaces = recents.filter((place, index) => recents.findIndex(other => placeKey(other) === placeKey(place)) === index);
  const recommendations = featuredPlaces.filter((place, index) => placeKey(place) !== (hero && placeKey(hero)) && !recentPlaces.some(recent => placeKey(recent) === placeKey(place)) && featuredPlaces.findIndex(other => placeKey(other) === placeKey(place)) === index);
  function visit(destination: LobbyDestination | null, place?: PlaceView, nearby = false) {
    if (!destination || jump.jumping) return;
    if (place) pushRecent(place);
    if (entry?.pending) {
      entry.enter("realm" in destination ? { kind: "world", realm: destination.realm }
        : { kind: "parcel", x: destination.x, y: destination.y });
      return;
    }
    if ("realm" in destination) {
      sendBridge("ChangeRealm", { realm: destination.realm });
      jump.beginJump(destination.name);
    } else {
      if (nearby) sendBridge("Teleport", { x: destination.x * 16 + 8, z: destination.y * 16 + 8 });
      else sendBridge("SendChat", { channel: "Nearby", message: `/goto ${destination.x},${destination.y}` });
      jump.beginJump(destination.name, `${destination.x},${destination.y}`);
    }
  }
  const placeButton = (place: PlaceView) => <PlaceCard key={`${place.world ? "world" : "land"}:${place.id}`} place={place} disabled={!placeDestination(place) || !!jump.jumping} onVisit={() => visit(placeDestination(place), place)} />;
  return <main className="lh" aria-label="Decentraland lobby">
    <FreshnessNotice failed={[featured, events, upcoming, results, worlds].some(query => query.refreshFailed)} onRetry={() => { for (const query of [featured, events, upcoming, results, worlds]) if (query.refreshFailed) void query.refetch(); }} />
    <header className="lh__header">
      <div className="lh__brand"><img className="lh__brand-mark" src={logo} alt="" /><span>Decentraland</span></div>
      <div className="lh__tools">
        <div className="lh__search-control"><button className="lh__search-toggle" aria-label="Search places and worlds" aria-expanded={searchOpen} onClick={() => setSearchOpen(value => !value)}><Search /></button>{searchOpen && <div className="lh__search"><Search /><input autoFocus type="search" aria-label="Search places and worlds" placeholder="Search places and worlds" value={search} maxLength={100} onChange={event => setSearch(event.target.value)} /><button aria-label="Clear search" onClick={() => { setSearch(""); setQuery(""); setSearchOpen(false); }}><Close /></button></div>}</div>
        <button className="lh__credits" data-panel-preload="marketplace" aria-label={credits.data ? `${credits.data} credits \u2014 open marketplace` : "Credits \u2014 open marketplace"} onClick={() => navigate("/marketplace")}><LobbyIcon name="credit" /><span>{credits.data ?? "Credits"}</span></button>
        <button className="lh__notification" data-panel-preload="notifications" aria-label={`Notifications${unread ? `, ${unread} unread` : ""}`} aria-expanded={headerMenu === "notifications"} onClick={() => setHeaderMenu(value => value === "notifications" ? null : "notifications")}><SidebarGlyph name="bell" />{unread > 0 && <span className="lh__unread">{unread > 99 ? "99+" : unread}</span>}</button>
        <button className="lh__profile" aria-label="Account menu" aria-expanded={headerMenu === "account"} onClick={() => setHeaderMenu(value => value === "account" ? null : "account")}><Avatar size={40} name={identity.name} src={avatarPreview || undefined} seed={identity.address || "guest"} /><strong>{identity.name || "Guest"}</strong></button>
      </div>
      {headerMenu && <LobbyHeaderMenu kind={headerMenu} onClose={() => setHeaderMenu(null)} onSignOut={onSignOut} />}

    </header>
    {search.trim() ? <section className="lh__results" aria-labelledby="lobby-search-title">
      <div className="lh__section-heading"><h2 id="lobby-search-title">Search results</h2><Button variant="ghost" size="sm" onClick={() => { setSearch(""); setQuery(""); }}>Back to lobby</Button></div>
      <p className="lh__note" role="status">{search.trim() !== query || results.isPending || worlds.isPending ? "Searching places and worlds\u2026" : `${(results.data?.length ?? 0) + (worlds.data?.length ?? 0)} results for \u201c${query}\u201d`}</p>
      {results.isError && <LoadState pending={false} error empty="" retry={() => results.refetch()} />}
      {worlds.isError && <LoadState pending={false} error empty="" retry={() => worlds.refetch()} />}
      <div className="lh__search-grid">{results.data?.map(placeButton)}{worlds.data?.map(placeButton)}</div>
      {!results.isPending && !worlds.isPending && !results.isError && !worlds.isError && !results.data?.length && !worlds.data?.length && <p className="lh__note">No matches. Try a different place or world name.</p>}
    </section> : <>
      <section className="lh__avatar" aria-label="Your avatar">
        <div className="lh__avatar-stage">
          <button type="button" className="lh__avatar-model" data-panel-preload={avatarStatus === "ready" ? "backpack" : undefined} aria-label={avatarStatus === "ready" ? "Edit avatar" : "Avatar preview loading"} disabled={avatarStatus !== "ready"} onClick={editAvatar}>
            {loadout && <WearablePreview key={avatarAttempt} base={catalystBase()} outfit={{
              bodyShape: look.bodyShape, wearables: look.wearables,
              skin: { color: hexToColor3(look.skinColor) }, hair: { color: hexToColor3(look.hairColor) }, eyes: { color: hexToColor3(look.eyeColor) },
            }} controls={false} yaw={-12} spin={false} platform={false} zoom={1.28} pitch={12} targetY={1.05} onStatus={setAvatarStatus} />}
            {avatarStatus === "ready" && <span className="lh__avatar-label" aria-hidden="true">Customize avatar</span>}
          </button>
        </div>
        {avatarStatus === "loading" && <p className="lh__avatar-status" role="status">Loading your avatar&#x2026;</p>}
        {(avatarStatus === "error" || avatarStatus === "empty") && <p className="lh__avatar-status" role="status">Avatar preview unavailable. <Button variant="ghost" size="sm" onClick={() => { setAvatarStatus("loading"); setAvatarAttempt(attempt => attempt + 1); }}>Retry preview</Button></p>}
      </section>
      <div className="lh__left">
        <section className="lh__welcome" aria-labelledby="lobby-welcome-title">
          <h1 id="lobby-welcome-title" ref={heading} tabIndex={-1}>{resuming ? <>You are in {scene.title || scene.realm || "Decentraland"}{(parcel || scene.coords) && <> &middot; {parcel || scene.coords}</>}</> : <>Welcome {identity.name || "Explorer"}!</>}</h1>
          {resuming ? <LobbySceneCard destination={{ realm: scene.realm ?? undefined, coords: parcel || scene.coords || undefined }} current={{ title: scene.title || scene.realm || "Decentraland", coords: parcel || scene.coords || "" }} onEnter={onEnterWorld} /> : linkedPreview || (entry?.pending && destination) ? <LobbySceneCard destination={target} onEnter={onEnterWorld} /> : hero ? <PlaceCard place={hero} hero onVisit={() => visit(placeDestination(hero), hero)} disabled={!placeDestination(hero) || !!jump.jumping} /> : <div className="lh__empty-card"><LoadState pending={featured.isPending} error={featured.isError} empty="Your next adventure is waiting." retry={() => featured.refetch()} /><button className="lh__jump" onClick={onEnterWorld}>Jump in <LobbyIcon name="jump" /></button></div>}
        </section>
        {friendState.friends.length > 0 && <section className="lh__social" aria-labelledby="lobby-friends-title">
          <div className="lh__section-heading"><h2 id="lobby-friends-title">Friends <small>{online.length} Online</small></h2><button className="lh__section-link" aria-label="All friends" data-panel-preload="friends" onClick={() => navigate("/friends")}>All friends</button></div>
          {online.length ? <LobbyCarousel label="Online friends" kind="friends">{online.map(friend => {
            const pin = pins.find(item => item.address.toLowerCase() === friend.address.toLowerCase());
            const destination = pin ? parcelDestination(friend.name, pin.coords) : null;
            return <div className="lh__friend" key={friend.address}><Avatar size={40} name={friend.name} src={friend.profilePictureUrl || undefined} seed={friend.address} /><strong>{friend.name}</strong><small><LobbyIcon name="pin" />{pin?.coords || "Location not shared"}</small><button className="lh__friend-join" disabled={!destination} title={destination ? `Join ${friend.name}` : "This friend\u2019s location is unavailable"} aria-label={`Join ${friend.name}`} onClick={() => visit(destination, undefined, true)}>Join <LobbyIcon name="jump" /></button></div>;
          })}</LobbyCarousel> : <div className="lh__empty-card lh__empty-card--friends"><p className="lh__note">{identity.isGuest ? "Sign in to find your friends here." : "No friends online right now."}</p><button className="lh__section-link" data-panel-preload="friends" onClick={() => navigate("/friends")}>Find your friends</button></div>}
        </section>}
      </div>
      <aside className="lh__right" aria-labelledby="lobby-events-title">
        <div className="lh__section-heading"><h2 id="lobby-events-title">Events</h2><button className="lh__section-link" data-panel-preload="events" onClick={() => navigate("/events")}>All events</button></div>
        {!liveEvents.length && <LoadState pending={events.isPending} error={events.isError} empty="No events live right now." retry={() => events.refetch()} />}
        {liveEvents[0] && <EventCard event={liveEvents[0]} onVisit={() => navigate("/events", { state: { event: liveEvents[0] } })} />}
        {!upcomingEvents.length && <LoadState pending={upcoming.isPending} error={upcoming.isError} empty="No upcoming events." retry={() => upcoming.refetch()} />}
        {upcomingEvents.length > 0 && <div className="lh__upcoming"><h3>Coming up</h3>{upcomingEvents.slice(0, 2).map(event => <EventCard key={event.id} event={event} compact onVisit={() => navigate("/events", { state: { event } })} />)}</div>}
      </aside>
      <div className={`lh__bottom${recentPlaces.length ? "" : " lh__bottom--new"}`}>
        <section aria-labelledby="lobby-recent-title"><div className="lh__section-heading"><h2 id="lobby-recent-title">Jump back in</h2></div>
          {recentPlaces.length ? <LobbyCarousel label="Recent places" kind="places">{recentPlaces.map(placeButton)}</LobbyCarousel> : <div className="lh__empty-card lh__empty-card--recent"><p className="lh__note">The places you visit will be waiting here.</p></div>}
        </section>
        <section aria-labelledby="lobby-featured-title"><div className="lh__section-heading"><h2 id="lobby-featured-title">Recommended places</h2><button className="lh__section-link" aria-label="Explore all places" data-panel-preload="places" onClick={() => navigate("/places")}>Explore all</button></div>
          {recommendations.length ? <LobbyCarousel label="Recommended places" kind="places">{recommendations.map(placeButton)}</LobbyCarousel> : <div className="lh__empty-card"><LoadState pending={featured.isPending} error={featured.isError} empty="Explore all places to find your next stop." retry={() => featured.refetch()} /></div>}
        </section>
      </div>
    </>}
    {jump.jumping && <JumpLoading name={jump.jumping} stalled={jump.stalled} onCancel={jump.cancelJump} onEnterAnyway={jump.confirmJump} />}
  </main>;
}
