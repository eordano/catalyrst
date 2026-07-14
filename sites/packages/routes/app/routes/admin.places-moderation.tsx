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

/**
 * Places moderation console.
 *
 * Read and writes are all admin-bearer gated on the server. Read directly in
 * this session:
 *
 *   catalyrst/crates/catalyrst-places/src/handlers/admin.rs:13-15  `gate()`
 *     -> catalyrst/crates/catalyrst-places/src/auth.rs:88-100  `require_admin_bearer`
 *        `expected: None`         -> 403 "Admin token not configured"
 *        bearer absent / mismatch -> 403 "Invalid admin credentials"
 *   `gate()` is the first statement of `get_reports` (admin.rs:41),
 *   `patch_report` (admin.rs:83) and `patch_place_disable` (admin.rs:131).
 *
 * Both directions therefore run server-side only:
 *   read  -- this loader calls `places-moderation.server.ts#loadReportQueue`.
 *   write -- the wizard posts to `/admin/places-decision`, whose action calls
 *           `places-moderation.server.ts#commitModerationDecision`.
 * `PLACES_ADMIN_AUTH_TOKEN` never enters the browser bundle.
 *
 * When the loader's answer is not `ok` the console is not rendered at all: the
 * page shows the server's reason. There is no sign-in button that reveals the
 * queue -- the previous `authGate -> queue` transition checked nothing and has
 * been removed from the machine.
 */
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
