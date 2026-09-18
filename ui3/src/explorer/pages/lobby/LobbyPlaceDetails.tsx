import { useEffect, useRef } from "react";
import type { PlaceView } from "../../../data/catalyst/places";
import { PlaceImage, LobbyIcon } from "./LobbyCards";
import { placeDestination } from "../lobbyDestinations";

export default function LobbyPlaceDetails({ place, onClose, onVisit }: { place: PlaceView; onClose: () => void; onVisit: () => void }) {
  const title = useRef<HTMLHeadingElement>(null);
  useEffect(() => { title.current?.focus({ preventScroll: true }); }, [place]);
  return <section className="lh__details lh__panel" aria-label="Place details">
    <div className="lh__detail-art"><PlaceImage src={place.image} /></div>
    <div className="lh__panel-heading"><h2 ref={title} tabIndex={-1}>{place.title}</h2><button className="lh__section-link" onClick={onClose}>Close details</button></div>
    <p className="lh__note">{place.creator} &#xb7; {place.world ? place.worldName : place.coords}</p>
    <p className="lh__description">{place.description || "Discover this place in Decentraland."}</p>
    <p className="lh__note">{place.players == null ? "Live population unavailable" : `${place.players} ${place.players === 1 ? "person" : "people"} here now`}</p>
    <button className="lh__jump" disabled={!placeDestination(place)} onClick={onVisit}>Jump in <LobbyIcon name="jump" /></button>
  </section>;
}
