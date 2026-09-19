import { Children, useEffect, useRef, useState, type ReactNode } from "react";
import DclLogomark from "../../../atoms/DclLogomark";
import type { PlaceView } from "../../../data/catalyst/places";
import type { DclEvent } from "../../../data/catalyst/events";
import { eventStart } from "../../../data/catalyst/events";

export function LobbyIcon({ name }: { name: "jump" | "people" | "pin" | "clock" | "live" | "credit" }) {
  return <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {name === "jump" && <><rect x="2" y="2" width="20" height="20" rx="6" /><path d="M7 12h10m-4-4 4 4-4 4" /></>}
    {name === "people" && <><circle cx="12" cy="7" r="4" /><path d="M4 21v-2a8 8 0 0 1 16 0v2Z" /></>}
    {name === "pin" && <><path d="M19 9c0 5-7 12-7 12S5 14 5 9a7 7 0 1 1 14 0Z" /><circle cx="12" cy="9" r="2" /></>}
    {name === "clock" && <><circle cx="12" cy="12" r="9" /><path d="M12 6v6h4" /></>}
    {name === "live" && <><circle cx="12" cy="12" r="2" fill="currentColor" /><path d="M7 7a7 7 0 0 0 0 10m10-10a7 7 0 0 1 0 10M4 4a11 11 0 0 0 0 16M20 4a11 11 0 0 1 0 16" /></>}
    {name === "credit" && <><path d="m12 1 9 5v12l-9 5-9-5V6Z" fill="#ff5265" stroke="#ffb185" /><path d="m16 8-4-2-5 3v6l5 3 4-2m-1-7-3-1-3 2v4l3 2 3-1" stroke="white" /></>}
  </svg>;
}

export function PlaceImage({ src }: { src?: string | null }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [src]);
  return src && !failed ? <img src={src} alt="" loading="lazy" onError={() => setFailed(true)} /> : <span className="lh__image-fallback"><DclLogomark size={36} /></span>;
}

export function LobbyCarousel({ label, kind, children }: { label: string; kind: "places" | "friends" | "events"; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState(0);
  const [pages, setPages] = useState(1);
  const count = Children.count(children);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      if (!el.clientWidth) return;
      const step = el.clientWidth + (parseFloat(getComputedStyle(el).columnGap) || 0);
      setPages(Math.max(1, Math.ceil((el.scrollWidth + (parseFloat(getComputedStyle(el).columnGap) || 0)) / step)));
      setPage(Math.round(el.scrollLeft / step));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    el.addEventListener("scroll", measure, { passive: true });
    return () => { observer.disconnect(); el.removeEventListener("scroll", measure); };
  }, [count]);
  return <div className="lh__carousel">
    <div ref={ref} className={`lh__rail lh__rail--${kind}`} aria-label={label}>{children}</div>
    {pages > 1 && <div className="lh__pagination" aria-label={`${label} pages`}>{Array.from({ length: pages }, (_, index) => <button key={index} type="button" aria-label={`${label}, page ${index + 1}`} aria-current={page === index ? "page" : undefined} onClick={() => {
      const el = ref.current;
      if (el) el.scrollTo({ left: index * (el.clientWidth + (parseFloat(getComputedStyle(el).columnGap) || 0)), behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
    }} />)}</div>}
  </div>;
}

export function Population({ count }: { count?: number | null }) {
  return count != null ? <span className="lh__population"><i /><LobbyIcon name="people" />{count}</span> : null;
}

export function PlaceCard({ place, hero = false, onVisit, disabled }: { place: PlaceView; hero?: boolean; onVisit: () => void; disabled?: boolean }) {
  return <button type="button" className={`lh__place${hero ? " lh__place--hero" : ""}`} disabled={disabled} onClick={onVisit} aria-label={`Jump in to ${place.title}`}>
    <PlaceImage src={place.image} />
    {hero && <div className="lh__card-badges"><Population count={place.players} /></div>}
    <span className="lh__place-copy"><strong>{place.title}</strong><small>{place.creator || (place.world ? "World" : "Decentraland")}</small></span>
    <span className="lh__jump" aria-hidden="true"><span>Jump in</span> <LobbyIcon name="jump" /></span>
  </button>;
}

export function EventCard({ event, compact = false, onVisit, disabled }: { event: DclEvent; compact?: boolean; onVisit: () => void; disabled?: boolean }) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 60000); return () => clearInterval(timer); }, []);
  const start = eventStart(event);
  const date = start ? new Date(start) : null;
  return <button type="button" className={`lh__event${compact ? " lh__event--compact" : ""}`} onClick={onVisit} disabled={disabled} aria-label={`View ${event.name || "event"}`}>
    <PlaceImage src={event.image} />
    {event.live && <span className="lh__card-badges"><span className="lh__live"><LobbyIcon name="live" />Live</span></span>}
    <span className="lh__event-copy"><strong>{event.name || "Event"}</strong><small>{event.user_name || event.scene_name || event.estate_name || "Decentraland"}</small>{compact && <span className="lh__event-time"><LobbyIcon name="clock" />{date && Number.isFinite(date.getTime()) ? relativeStart(date.getTime(), now) : "Details"}</span>}</span>
  </button>;
}

export function relativeStart(start: number, now: number): string {
  const minutes = Math.ceil((start - now) / 60000);
  if (minutes <= 0) return "Starting now";
  if (minutes < 60) return `In ${minutes} ${minutes === 1 ? "minute" : "minutes"}`;
  const hours = Math.ceil(minutes / 60);
  if (hours < 24) return `In ${hours} ${hours === 1 ? "hour" : "hours"}`;
  const days = Math.ceil(hours / 24);
  return `In ${days} ${days === 1 ? "day" : "days"}`;
}
