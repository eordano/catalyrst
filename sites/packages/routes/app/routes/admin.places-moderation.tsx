import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import AdControlNotice from "@ui/admin/pages/AdControlNotice";
import AdPlacesModerationPage from "@ui/admin/pages/AdPlacesModerationPage";
import SitesChrome from "@ui/web/frames/SitesChrome";

import {
  REPORT_REASONS,
  RESOLUTION_OPTIONS,
  toReportCard,
  type Option,
  type ReportCard,
  type ReportRow,
} from "@data/lib/catalyst/admin/places-moderation";
import { loadReportQueue } from "@data/lib/catalyst/admin/places-moderation.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import ModeratePlacesWizard from "@features/stories/admin/places-moderation/ModeratePlacesWizard";

import type { Route } from "./+types/admin.places-moderation";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/places-moderation";

const FALLBACK: Assignment = {
  variant: "bucketed_queue",
  flags: { bucketed_queue: true },
  experimentKey: "admin_place_moderation_queue",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const queue = await loadReportQueue({
    signal: request.signal,
    status: "open",
    limit: 50,
  });

  const reports: ReportRow[] = queue.ok ? queue.data.rows : [];
  const total = queue.ok ? queue.data.total : 0;
  const unavailable = queue.ok ? null : queue;

  const cards: ReportCard[] = reports.map(toReportCard);
  const reasons: Option[] = REPORT_REASONS;
  const resolutions: Option[] = RESOLUTION_OPTIONS;

  const payload = {
    sid,
    step,
    reports,
    cards,
    reasons,
    resolutions,
    total,
    unavailable,
    assignment,
  };

  return wrap(payload);
}

export default function AdminPlacesModerationRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <SitesChrome active="play">
      <AdPlacesModerationPage
        nav={
          <>
            <Link prefetch="intent" to={href("/admin/places-moderation")} aria-current="page">
              Places
            </Link>
            <Link prefetch="intent" to={href("/admin/communities-moderation")}>
              Communities
            </Link>
            <Link prefetch="intent" to={href("/admin/whatson-users")}>
              What's On
            </Link>
            <Link prefetch="intent" to={href("/admin/metrics")}>
              Metrics
            </Link>
          </>
        }
      >
        {d.unavailable ? (
          <AdControlNotice
            title="Places moderation is unavailable on this node"
            message={d.unavailable.message}
            status={d.unavailable.status}
            serverCheck={d.unavailable.serverCheck}
            fix={d.unavailable.fix}
          />
        ) : (
          <ModeratePlacesWizard
            trackCtx={{
              sid: d.sid,
              story: STORY,
              variant: d.assignment.variant,
              experimentKey: d.assignment.experimentKey,
            }}
            reports={d.reports}
            cards={d.cards}
            reasons={d.reasons}
            resolutions={d.resolutions}
            total={d.total}
            initialStep={d.step ?? undefined}
          />
        )}
      </AdPlacesModerationPage>
    </SitesChrome>
  );
}
