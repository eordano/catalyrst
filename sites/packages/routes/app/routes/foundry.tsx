import { useCallback, useEffect, useRef } from "react";
import { Outlet, useLocation, useNavigate, useSearchParams } from "react-router";

import Button from "@ui/atoms/Button";
import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import FdTourCard from "@ui/foundry/components/FdTourCard";

import "@ui/atoms/button.css";
import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import FdRoomDock, { type FdRoomTicket } from "@ui/foundry/components/FdRoomDock";
import "@ui/foundry/components/fdroomdock.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/foundry/components/fdtourcard.css";
import "@ui/foundry/components/fdsection.css";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

import { sidBadge } from "@data/lib/foundry/db.server";

import type { Route } from "./+types/foundry";

const STORY: StoryId = "foundry/tour-activation";

const FALLBACK: Assignment = {
  variant: "control",
  flags: {},
  experimentKey: "foundry-tour-activation",
};

type FoundryTabId =
  | "overview"
  | "select"
  | "persona"
  | "people"
  | "timeline"
  | "sessions"
  | "continuity"
  | "stewardship"
  | "play"
  | "gdd"
  | "deck"
  | "copilot"
  | "exchange"
  | "console";

const TABS: readonly { id: FoundryTabId; label: string; href: string }[] = [
  { id: "overview", label: "Overview", href: "/foundry" },
  { id: "select", label: "Start here", href: "/foundry/select" },
  { id: "play", label: "Play", href: "/foundry/play" },
  { id: "gdd", label: "Design docs", href: "/foundry/gdd" },
  { id: "deck", label: "Deck", href: "/foundry/deck" },
  { id: "exchange", label: "Exchange", href: "/foundry/exchange" },
  { id: "people", label: "People", href: "/foundry/people" },
  { id: "sessions", label: "Sessions", href: "/foundry/sessions" },
  { id: "timeline", label: "Timeline", href: "/foundry/timeline" },
  { id: "persona", label: "Persona", href: "/foundry/persona" },
  { id: "continuity", label: "Continuity", href: "/foundry/continuity" },
  { id: "stewardship", label: "Stewardship", href: "/foundry/stewardship" },
  { id: "copilot", label: "Copilot", href: "/foundry/copilot" },
  { id: "console", label: "Console", href: "/foundry/console/bench" },
];

type TourStep = {
  id: string;
  title: string;
  body: string;
  path: string;
  query?: Record<string, string>;
};

// Journey-ordered to match the nav (games → gdd → the society surfaces →
// console), with the exchange kept as the closer: the tour's last stop is the
// ask. Step ids are stable so telemetry step_id stays comparable across orders.
const TOUR: readonly TourStep[] = [
  {
    id: "games",
    title: "Walk into the games",
    body:
      "Games by Decentraland creators, deployed to Worlds — each game page carries the world it lives in, its size, the deployment entity its date is read from, and a link straight into the client.",
    path: "/foundry/play",
  },
  {
    id: "gdd",
    title: "Design docs, marks and all",
    body:
      "shortGDDs in the Creator Success format. Open questions stay marked TBD, and the page counts those markers section by section.",
    path: "/foundry/gdd",
  },
  {
    id: "people",
    title: "Who is here",
    body:
      "Anyone can claim a name on the persona page and appear in the directory. Host and operator roles are granted by invite code and listed with the consent that put them there.",
    path: "/foundry/people",
  },
  {
    id: "sessions",
    title: "When this place gathers",
    body:
      "Hosts put sessions on the calendar; weekly series repeat for the next 28 days. RSVP to say you'll come — the count on each session is everyone who did.",
    path: "/foundry/sessions",
  },
  {
    id: "timeline",
    title: "What this place remembers",
    body:
      "Deployments, bench runs, design docs, and what visitors do here, newest first. Filter by lane to see just the community's own rows.",
    path: "/foundry/timeline",
  },
  {
    id: "continuity",
    title: "What you can take with you",
    body:
      "Every scene's record — changelog, runs, docs, stewards — exports as a JSON bundle you can download and carry elsewhere.",
    path: "/foundry/continuity",
  },
  {
    id: "copilot",
    title: "A copilot we host",
    body:
      "A self-hosted opencode instance wired to our own model gateway. Every token it spends is counted, and the costs page carries the full ledger.",
    path: "/foundry/copilot",
  },
  {
    id: "bench",
    title: "Bots test the games",
    body:
      "Bots play the games through the dcl-scene-bots harness. Each run's verdict and full event log are on its card; a check that cannot be evaluated fails.",
    path: "/foundry/console/bench",
  },
  {
    id: "trajectories",
    title: "Every run is a log",
    body:
      "Each run is an append-only event log. Open an episode and step through it, event by event.",
    path: "/foundry/console/trajectories",
  },
  {
    id: "costs",
    title: "Costs are labeled",
    body:
      "Token counts are measured; the dollar figure is labeled reference pricing. It is a chosen constant for a self-hosted model, not a bill anyone sent us.",
    path: "/foundry/console/costs",
  },
  {
    id: "exchange",
    title: "Ask for the next one",
    body:
      "Ask for what you want built; pledge on requests other people posted. Your pledge appears to every other visitor the moment you make it.",
    path: "/foundry/exchange",
  },
];

function activeTab(pathname: string): FoundryTabId {
  if (pathname.startsWith("/foundry/console")) return "console";
  if (pathname.startsWith("/foundry/select")) return "select";
  if (pathname.startsWith("/foundry/persona")) return "persona";
  if (pathname.startsWith("/foundry/people")) return "people";
  if (pathname.startsWith("/foundry/timeline")) return "timeline";
  if (pathname.startsWith("/foundry/sessions")) return "sessions";
  if (pathname.startsWith("/foundry/continuity")) return "continuity";
  if (pathname.startsWith("/foundry/stewardship")) return "stewardship";
  if (pathname.startsWith("/foundry/play")) return "play";
  if (pathname.startsWith("/foundry/gdd")) return "gdd";
  if (pathname.startsWith("/foundry/deck")) return "deck";
  if (pathname.startsWith("/foundry/copilot")) return "copilot";
  if (pathname.startsWith("/foundry/exchange")) return "exchange";
  return "overview";
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK, {
    skipExposure: true,
  });
  return wrap({
    badge: sidBadge(sid),
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });
}

export default function FoundryLayout({ loaderData }: Route.ComponentProps) {
  const { badge, variant, experimentKey } = loaderData;
  const location = useLocation();
  const navigate = useNavigate();
  const [params] = useSearchParams();

  const raw = Number.parseInt(params.get("tour") ?? "", 10);
  const step = Number.isFinite(raw) && raw >= 1 && raw <= TOUR.length ? raw : 0;
  const active = step > 0;
  const source = params.get("tour_src") === "auto" ? "auto" : "button";
  const ctx = { sid: badge, story: STORY, variant, experimentKey };

  const goToStep = useCallback(
    (n: number) => {
      const target = TOUR[n - 1];
      if (!target) return;
      const search = new URLSearchParams({ ...target.query, tour: String(n) });
      // A preview session states its arm in the URL; every tour hop has to carry it
      // or the rest of the session is labeled with the sid's hashed arm instead.
      const variantParam = params.get("variant");
      if (variantParam) search.set("variant", variantParam);
      navigate(`${target.path}?${search.toString()}`);
    },
    [navigate, params],
  );

  const clearTour = useCallback(() => {
    const next = new URLSearchParams(params);
    next.delete("tour");
    next.delete("tour_src");
    const search = next.toString();
    navigate(
      { pathname: location.pathname, search: search ? `?${search}` : "" },
      { replace: true, preventScrollReset: true },
    );
  }, [location.pathname, navigate, params]);

  const onEnd = useCallback(() => {
    track("fd_tour_dismissed", { step, steps: TOUR.length }, ctx);
    clearTour();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [clearTour, step, badge, variant, experimentKey]);

  const onNext = useCallback(() => {
    if (step >= TOUR.length) {
      track("fd_tour_completed", { steps: TOUR.length }, ctx);
      clearTour();
      return;
    }
    goToStep(step + 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [clearTour, goToStep, step, badge, variant, experimentKey]);

  const onBack = useCallback(() => {
    if (step <= 1) return;
    goToStep(step - 1);
  }, [goToStep, step]);

  const started = useRef(false);
  useEffect(() => {
    if (!active) {
      started.current = false;
      return;
    }
    if (started.current) return;
    started.current = true;
    track(
      "fd_tour_started",
      { page: location.pathname, source, steps: TOUR.length },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, badge]);

  useEffect(() => {
    if (!active) return;
    const current = TOUR[step - 1];
    track(
      "fd_tour_step_viewed",
      { page: location.pathname, step, step_id: current.id, steps: TOUR.length },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, step, badge]);

  // URL entry (or the auto-start navigate) can land a tour step on a page that
  // is not the step's own; the card must sit over the page it describes.
  useEffect(() => {
    if (!active) return;
    const target = TOUR[step - 1];
    if (location.pathname.startsWith(target.path)) return;
    navigate(`${target.path}${location.search}`, { replace: true });
  }, [active, step, location.pathname, location.search, navigate]);

  const current = active ? TOUR[step - 1] : null;

  return (
    <ChromeShell
      className="fd"
      ariaLabel="The Foundry"
      topbar={<DclTopBar variant="sites" active="" />}
      brand="Foundry"
      brandHref="/foundry"
      tabs={TABS}
      active={activeTab(location.pathname)}
      onNavigate={(href) => navigate(href)}
      tabsLabel="Foundry sections"
      footer={false}
      right={
        <Button variant="ghost" size="sm" onClick={() => goToStep(1)}>
          Take the tour
        </Button>
      }
    >
      <Outlet />
      <div className="fd-corner">
        <FdRoomDock
          path={location.pathname}
          getTicket={fetchRoomTicket}
          onEvent={(event, others) => {
            const name =
              event === "joined"
                ? ("fd_room_joined" as const)
                : event === "message_sent"
                  ? ("fd_room_message_sent" as const)
                  : ("fd_room_mic_on" as const);
            track(name, { others, path: location.pathname }, ctx);
          }}
        />
        {current ? (
          <FdTourCard
            step={step}
            total={TOUR.length}
            title={current.title}
            body={current.body}
            onNext={onNext}
            onBack={onBack}
            onEnd={onEnd}
          />
        ) : null}
      </div>
    </ChromeShell>
  );
}

async function fetchRoomTicket(path: string): Promise<FdRoomTicket | null> {
  try {
    const form = new FormData();
    form.set("path", path);
    const res = await fetch("/foundry/room-token", { method: "POST", body: form });
    if (!res.ok) return null;
    const body = (await res.json()) as (FdRoomTicket & { ok?: boolean }) | null;
    return body?.ok ? body : null;
  } catch {
    return null;
  }
}
