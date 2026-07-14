import { useEffect, useRef } from "react";

import PlaceCard from "@ui/components/PlaceCard";
import "@ui/explorer/pages/places.css";

import UpstreamUnavailable from "../../../components/UpstreamUnavailable";
import { mapPlace, type Place } from "@data/lib/catalyst/places/index";
import {
  OPEN_SCREEN_TARGETS,
  placeJumpPath,
  type OpenScreenArm,
} from "@core/lib/experiments/open-screen";
import {
  track as defaultTrack,
  type TrackContext,
  type TrackFn,
} from "@core/lib/telemetry/track";

export { toOpenPlace } from "./select";
import { type OpenPlace } from "./select";

export type OpenCardTarget = "avatar" | "genesis" | "random";

export type OpenScreenProps = {
  arm: OpenScreenArm;
  places: Place[] | null;
  busiest: OpenPlace | null;
  surprise: OpenPlace | null;
  trackCtx: TrackContext;
  track?: TrackFn;
  navigate?: (url: string) => void;
  jumpDelayMs?: number;
};

function defaultNavigate(url: string): void {
  if (typeof window !== "undefined") window.location.assign(url);
}

export default function OpenScreen({
  arm,
  places,
  busiest,
  surprise,
  trackCtx,
  track = defaultTrack,
  navigate = defaultNavigate,
  jumpDelayMs = 2500,
}: OpenScreenProps) {
  const shownRef = useRef(false);
  useEffect(() => {
    if (shownRef.current) return;
    shownRef.current = true;
    track("cl_open_screen_shown", { variant: arm }, trackCtx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const emitJumpedIn = (placeId: string) =>
    track("cl_open_jumped_in", { place_id: placeId, variant: arm }, trackCtx);

  switch (arm) {
    case "genesis":
      return (
        <GenesisSpawn
          busiest={busiest}
          trackCtx={trackCtx}
          track={track}
          navigate={navigate}
          jumpDelayMs={jumpDelayMs}
          emitJumpedIn={emitJumpedIn}
        />
      );
    case "three-cards":
      return (
        <ThreeCards
          busiest={busiest}
          surprise={surprise}
          trackCtx={trackCtx}
          track={track}
          navigate={navigate}
          emitJumpedIn={emitJumpedIn}
        />
      );
    default:
      return (
        <BaseExplore
          places={places}
          trackCtx={trackCtx}
          track={track}
          navigate={navigate}
          emitJumpedIn={emitJumpedIn}
        />
      );
  }
}

function placeLabel(p: OpenPlace): string {
  return p.title?.trim() || p.base_position;
}

function BaseExplore({
  places,
  trackCtx,
  track,
  navigate,
  emitJumpedIn,
}: {
  places: Place[] | null;
  trackCtx: TrackContext;
  track: TrackFn;
  navigate: (url: string) => void;
  emitJumpedIn: (placeId: string) => void;
}) {
  const openedRef = useRef(false);
  useEffect(() => {
    if (openedRef.current || places === null) return;
    openedRef.current = true;
    track("cl_explore_opened", { place_count: places.length }, trackCtx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [places]);

  if (places === null) {
    return (
      <UpstreamUnavailable
        title="Places are temporarily unavailable"
        message="We couldn't load the things-to-do list. Please try again in a moment."
        backHref={OPEN_SCREEN_TARGETS.explore}
        backLabel="Open Places"
      />
    );
  }

  function onCardClick(e: React.MouseEvent, placeId: string) {
    e.preventDefault();
    track("place_card_clicked", { place_id: placeId }, trackCtx);
    emitJumpedIn(placeId);
    navigate(placeJumpPath(placeId));
  }

  return (
    <section style={SCREEN} aria-label="Things to do">
      <header style={HEAD}>
        <h1 style={TITLE}>Things to do</h1>
        <a
          href={OPEN_SCREEN_TARGETS.explore}
          onClick={(e) => {
            e.preventDefault();
            navigate(OPEN_SCREEN_TARGETS.explore);
          }}
          style={SUBLINK}
        >
          Browse all places &#x2192;
        </a>
      </header>
      {places.length > 0 ? (
        <div className="pl__grid">
          {places.map((p, i) => {
            const card = mapPlace(p);
            return (
              <a
                key={card.id}
                href={placeJumpPath(card.id)}
                onClick={(e) => onCardClick(e, card.id)}
                style={{ display: "block", textDecoration: "none", color: "inherit" }}
              >
                <PlaceCard
                  title={card.title}
                  image={card.image}
                  players={card.players}
                  rating={card.rating}
                  coords={card.coords}
                  live={card.live ? 1 : undefined}
                  featured={card.featured}
                  creator={card.creator}
                  hue={(i * 47) % 360}
                  presentational
                />
              </a>
            );
          })}
        </div>
      ) : (
        <p style={MUTED}>No places found.</p>
      )}
    </section>
  );
}

function GenesisSpawn({
  busiest,
  trackCtx,
  track,
  navigate,
  jumpDelayMs,
  emitJumpedIn,
}: {
  busiest: OpenPlace | null;
  trackCtx: TrackContext;
  track: TrackFn;
  navigate: (url: string) => void;
  jumpDelayMs: number;
  emitJumpedIn: (placeId: string) => void;
}) {
  const spawnRef = useRef(false);
  useEffect(() => {
    if (spawnRef.current || !busiest) return;
    spawnRef.current = true;
    track(
      "cl_open_genesis_spawn",
      { place_id: busiest.id, user_count: busiest.user_count },
      trackCtx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [busiest]);

  const jumpedRef = useRef(false);
  const jump = () => {
    if (jumpedRef.current || !busiest) return;
    jumpedRef.current = true;
    emitJumpedIn(busiest.id);
    navigate(placeJumpPath(busiest.id));
  };
  // Opting out (browse instead) must cancel the pending auto-jump: otherwise a
  // timer that fires mid-navigation would override the user's choice and fling
  // them into the scene they explicitly declined. Setting jumpedRef makes the
  // scheduled jump() a no-op -- the same guard "Jump now" already relies on.
  const cancelAutoJump = () => {
    jumpedRef.current = true;
  };
  const jumpRef = useRef(jump);
  jumpRef.current = jump;

  useEffect(() => {
    if (!busiest) return;
    const t = setTimeout(() => jumpRef.current(), jumpDelayMs);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [busiest, jumpDelayMs]);

  // No live reading: never dead-end on a "browse instead" prompt. The loader
  // redirects server-side; this covers the client-only render (Storybook, edge
  // cases) so the player still lands on Places with zero clicks.
  const redirectedRef = useRef(false);
  useEffect(() => {
    if (busiest || redirectedRef.current) return;
    redirectedRef.current = true;
    navigate(OPEN_SCREEN_TARGETS.explore);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [busiest]);

  if (!busiest) {
    return (
      <section style={{ ...SCREEN, ...CENTER }} aria-label="Taking you to Places">
        <p style={MUTED}>Taking you to Places&#x2026;</p>
      </section>
    );
  }

  return (
    <section style={{ ...SCREEN, ...CENTER }} aria-label={`Now entering ${placeLabel(busiest)}`}>
      <p style={KICKER}>Where the action is</p>
      <h1 style={BIG_TITLE}>Now entering: {placeLabel(busiest)}</h1>
      <div style={SPAWN_CARD}>
        <div style={SPAWN_META}>
          {busiest.user_count} online now &#xB7; {busiest.base_position}
        </div>
      </div>
      <div style={ACTIONS}>
        <button type="button" onClick={jump} style={CTA_PRIMARY}>
          Jump now
        </button>
        <a
          href={OPEN_SCREEN_TARGETS.explore}
          onClick={(e) => {
            e.preventDefault();
            cancelAutoJump();
            navigate(OPEN_SCREEN_TARGETS.explore);
          }}
          style={CTA_SECONDARY}
        >
          Let me browse instead
        </a>
      </div>
      <p style={MUTED}>Jumping automatically&#x2026;</p>
    </section>
  );
}

function ThreeCards({
  busiest,
  surprise,
  trackCtx,
  track,
  navigate,
  emitJumpedIn,
}: {
  busiest: OpenPlace | null;
  surprise: OpenPlace | null;
  trackCtx: TrackContext;
  track: TrackFn;
  navigate: (url: string) => void;
  emitJumpedIn: (placeId: string) => void;
}) {
  function onCard(
    e: React.MouseEvent,
    target: OpenCardTarget,
    place: OpenPlace | null,
    href: string,
  ) {
    e.preventDefault();
    track("cl_open_card_clicked", { target }, trackCtx);
    if (place) emitJumpedIn(place.id);
    navigate(href);
  }

  return (
    <section style={{ ...SCREEN, ...CENTER }} aria-label="Main screen">
      <h1 style={BIG_TITLE}>What do you feel like?</h1>
      <div style={CARD_ROW}>
        <ChooserCard
          title="Jump into the action"
          subtitle={
            busiest
              ? `${placeLabel(busiest)} \u{2014} ${busiest.user_count} online now`
              : "No live scene reading right now"
          }
          href={busiest ? placeJumpPath(busiest.id) : null}
          onClick={(e) => onCard(e, "genesis", busiest, placeJumpPath(busiest?.id ?? ""))}
        />
        <ChooserCard
          title="Surprise me"
          subtitle={
            surprise
              ? "A random live scene with players in it"
              : "No live scene reading right now"
          }
          href={surprise ? placeJumpPath(surprise.id) : null}
          onClick={(e) => onCard(e, "random", surprise, placeJumpPath(surprise?.id ?? ""))}
        />
        <ChooserCard
          title="Customize your avatar"
          subtitle="Open your backpack and make the look yours"
          href={OPEN_SCREEN_TARGETS.avatar}
          onClick={(e) => onCard(e, "avatar", null, OPEN_SCREEN_TARGETS.avatar)}
        />
      </div>
      <a
        href={OPEN_SCREEN_TARGETS.explore}
        onClick={(e) => {
          e.preventDefault();
          navigate(OPEN_SCREEN_TARGETS.explore);
        }}
        style={SUBLINK}
      >
        Browse all places &#x2192;
      </a>
    </section>
  );
}

function ChooserCard({
  title,
  subtitle,
  href,
  onClick,
}: {
  title: string;
  subtitle: string;
  href: string | null;
  onClick: (e: React.MouseEvent) => void;
}) {
  if (!href) {
    return (
      <div style={{ ...CHOOSER, ...CHOOSER_DISABLED }} aria-disabled="true">
        <div style={CHOOSER_TITLE}>{title}</div>
        <div style={CHOOSER_SUB}>{subtitle}</div>
      </div>
    );
  }
  return (
    // No aria-label: the accessible name is composed from the visible title +
    // subtitle so a screen reader hears the differentiating context (the place
    // name / online count), not just the generic card title.
    <a href={href} onClick={onClick} style={CHOOSER}>
      <div style={CHOOSER_TITLE}>{title}</div>
      <div style={CHOOSER_SUB}>{subtitle}</div>
    </a>
  );
}

const SCREEN: React.CSSProperties = {
  maxWidth: 1080,
  margin: "0 auto",
  padding: "32px 24px 48px",
  color: "var(--text, #fff)",
};
const CENTER: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  gap: 20,
  textAlign: "center",
  paddingTop: 72,
};
const HEAD: React.CSSProperties = {
  display: "flex",
  alignItems: "baseline",
  justifyContent: "space-between",
  gap: 16,
  margin: "0 0 20px",
};
const TITLE: React.CSSProperties = { margin: 0, fontSize: 26, lineHeight: 1.2 };
const BIG_TITLE: React.CSSProperties = { margin: 0, fontSize: 32, lineHeight: 1.2 };
const KICKER: React.CSSProperties = {
  margin: 0,
  fontSize: 13,
  fontWeight: 700,
  letterSpacing: "0.08em",
  textTransform: "uppercase",
  color: "var(--brand, #ff2d55)",
};
const SUBLINK: React.CSSProperties = {
  color: "var(--ink-7, rgba(255,255,255,0.7))",
  fontSize: 14,
  textDecoration: "none",
};
const MUTED: React.CSSProperties = {
  color: "var(--ink-6, rgba(255,255,255,0.6))",
  fontSize: 14,
  margin: 0,
  padding: "8px 0",
};
const SPAWN_CARD: React.CSSProperties = {
  minWidth: 280,
  padding: "18px 26px",
  border: "1px solid var(--line, rgba(255,255,255,0.15))",
  borderRadius: "var(--r-card, 12px)",
  background: "color-mix(in srgb, var(--brand, #ff2d55) 8%, var(--fill-1, rgba(255,255,255,0.06)))",
};
const SPAWN_META: React.CSSProperties = {
  fontSize: 15,
  fontWeight: 600,
  color: "var(--ink-8, rgba(255,255,255,0.82))",
};
const ACTIONS: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  flexWrap: "wrap",
  justifyContent: "center",
  gap: 10,
};
const CTA_BASE: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  borderRadius: "var(--r-control, 8px)",
  padding: "10px 18px",
  fontSize: 14,
  fontWeight: 600,
  textDecoration: "none",
  cursor: "pointer",
  border: "none",
};
const CTA_PRIMARY: React.CSSProperties = {
  ...CTA_BASE,
  background: "var(--brand, #ff2d55)",
  color: "var(--on-brand, #fff)",
};
const CTA_SECONDARY: React.CSSProperties = {
  ...CTA_BASE,
  border: "1px solid var(--line, rgba(255,255,255,0.15))",
  background: "var(--fill-2, rgba(255,255,255,0.08))",
  color: "var(--text, #fff)",
};
const CARD_ROW: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))",
  gap: 16,
  width: "100%",
  maxWidth: 920,
};
const CHOOSER: React.CSSProperties = {
  display: "block",
  padding: "26px 22px",
  border: "1px solid var(--line, rgba(255,255,255,0.15))",
  borderRadius: "var(--r-card, 12px)",
  background: "var(--panel, rgba(255,255,255,0.04))",
  color: "inherit",
  textDecoration: "none",
  textAlign: "left",
  minHeight: 120,
};
const CHOOSER_DISABLED: React.CSSProperties = {
  opacity: 0.55,
  cursor: "not-allowed",
};
const CHOOSER_TITLE: React.CSSProperties = { fontSize: 18, fontWeight: 700 };
const CHOOSER_SUB: React.CSSProperties = {
  marginTop: 6,
  fontSize: 14,
  lineHeight: 1.45,
  color: "var(--ink-7, rgba(255,255,255,0.7))",
};
