import { useCallback, useEffect, useRef } from "react";
import { Outlet, useLocation, useNavigate, useSearchParams } from "react-router";

import Button from "@ui/atoms/Button";
import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import { FdDataNote } from "@ui/foundry/components/FdSection";
import FdTourCard from "@ui/foundry/components/FdTourCard";

import "@ui/atoms/button.css";
import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/foundry/components/fdtourcard.css";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { foundryProvenance } from "@data/lib/foundry/program.server";
import type { FdProvenance } from "@ui/foundry/components/FdSection";

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
  | "play"
  | "gdd"
  | "copilot"
  | "exchange"
  | "console";

const TABS: readonly { id: FoundryTabId; label: string; href: string }[] = [
  { id: "overview", label: "Overview", href: "/foundry" },
  { id: "select", label: "Start here", href: "/foundry/select" },
  { id: "play", label: "Play", href: "/foundry/play" },
  { id: "gdd", label: "Design docs", href: "/foundry/gdd" },
  { id: "copilot", label: "Copilot", href: "/foundry/copilot" },
  { id: "exchange", label: "Exchange", href: "/foundry/exchange" },
  { id: "console", label: "Console", href: "/foundry/console/bench" },
];

type TourStep = {
  id: string;
  title: string;
  body: string;
  path: string;
  query?: Record<string, string>;
};

const TOUR: readonly TourStep[] = [
  {
    id: "games",
    title: "The games are real",
    body:
      "Seven real games from Decentraland creators. Dates come from the deployment entities, not from us — each game page carries the world it lives in, how big it is, and a link straight into the client.",
    path: "/foundry/play",
  },
  {
    id: "gdd",
    title: "Design docs, marks and all",
    body:
      "shortGDDs in the Creator Success format. Open questions stay marked TBD instead of being papered over, and the page counts those markers rather than hiding them.",
    path: "/foundry/gdd",
  },
  {
    id: "copilot",
    title: "A copilot we host",
    body:
      "A self-hosted opencode instance wired to our own model gateway. Every token it spends is counted and shown, and the page states plainly which of its skills work today.",
    path: "/foundry/copilot",
  },
  {
    id: "bench",
    title: "Bots test the games",
    body:
      "Bots test the games through the dcl-scene-bots harness. A check that cannot be evaluated fails, and a run that never happened is shown as nothing at all.",
    path: "/foundry/console/bench",
  },
  {
    id: "trajectories",
    title: "Every run is a log",
    body:
      "Each run is an append-only event log. Scrub back through any episode: replay is re-derivation, so the scrubber shows exactly what the log contains and nothing more.",
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
      "Ask for what you want built; pledge on others' requests. Counts are pure row counts, and your pledge is visible to every other visitor the moment you make it.",
    path: "/foundry/exchange",
  },
];

function activeTab(pathname: string): FoundryTabId {
  if (pathname.startsWith("/foundry/console")) return "console";
  if (pathname.startsWith("/foundry/select")) return "select";
  if (pathname.startsWith("/foundry/play")) return "play";
  if (pathname.startsWith("/foundry/gdd")) return "gdd";
  if (pathname.startsWith("/foundry/copilot")) return "copilot";
  if (pathname.startsWith("/foundry/exchange")) return "exchange";
  return "overview";
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK, {
    skipExposure: true,
  });
  // The standing note renders these real figures instead of asserting them.
  // A missing database is a fact the note states, not an error the chrome throws.
  let provenance: FdProvenance | null = null;
  try {
    provenance = await foundryProvenance(getPool());
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
  }
  return wrap({
    badge: sidBadge(sid),
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
    provenance,
  });
}

export default function FoundryLayout({ loaderData }: Route.ComponentProps) {
  const { badge, variant, experimentKey, provenance } = loaderData;
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

  const current = active ? TOUR[step - 1] : null;

  return (
    <ChromeShell
      className="fd"
      ariaLabel="The Foundry"
      topbar={<DclTopBar variant="sites" active="" />}
      brand="Foundry"
      tabs={TABS}
      active={activeTab(location.pathname)}
      tabsLabel="Foundry sections"
      right={
        <Button variant="ghost" size="sm" onClick={() => goToStep(1)}>
          Take the tour
        </Button>
      }
    >
      <Outlet />
      <FdDataNote provenance={provenance} />
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
    </ChromeShell>
  );
}
