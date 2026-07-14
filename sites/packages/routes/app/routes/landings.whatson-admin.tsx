import LdWhatsOnAdminPage from "@ui/landings/pages/LdWhatsOnAdminPage";

import {
  bucketOf,
  fetchModerationQueue,
  fetchModerationPending,
  toAdminCard,
  REJECT_REASONS,
  type AdminEventCard,
  type ModeratableEvent,
  type RejectReason,
} from "@data/lib/catalyst/admin/whatson-admin";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import ModerateWizard from "@features/stories/landings/whatson-admin-moderate/ModerateWizard";

import type { Route } from "./+types/landings.whatson-admin";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/whatson-admin-moderate";

type QueueCards = {
  pending: AdminEventCard[];
  approved: AdminEventCard[];
  featured: AdminEventCard[];
};

type QueueFallback = {
  live: boolean;
  pending: boolean;
};

function bucketCards(events: ModeratableEvent[], now: Date): QueueCards {
  const out: QueueCards = { pending: [], approved: [], featured: [] };
  for (const e of events) out[bucketOf(e)].push(toAdminCard(e, now));
  return out;
}

const FALLBACK: Assignment = {
  variant: "moderation_wizard",
  flags: { moderation_wizard: true },
  experimentKey: "lp_whatson_admin_moderation",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const now = new Date();

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const cards: QueueCards = { pending: [], approved: [], featured: [] };
  const fallback: QueueFallback = { live: false, pending: false };

  const [publicRes, pendingRes] = await Promise.allSettled([
    fetchModerationQueue({ signal: request.signal, limit: 24 }),
    fetchModerationPending({ signal: request.signal, limit: 24 }),
  ]);

  if (publicRes.status === "fulfilled") {
    const live = bucketCards(publicRes.value, now);
    cards.approved = live.approved;
    cards.featured = live.featured;
    cards.pending.push(...live.pending);
  } else {
    fallback.live = true;
  }

  if (pendingRes.status === "fulfilled") {
    for (const e of pendingRes.value) cards.pending.push(toAdminCard(e, now));
  } else {
    fallback.pending = true;
  }

  const rejectReasons: RejectReason[] = REJECT_REASONS;

  const payload = {
    sid,
    cards,
    rejectReasons,
    assignment,
    fallback,
  };

  return wrap(payload);
}

export default function WhatsOnAdminRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <LdWhatsOnAdminPage>
      <ModerateWizard
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        pending={d.cards.pending}
        approved={d.cards.approved}
        featured={d.cards.featured}
        rejectReasons={d.rejectReasons}
      />
    </LdWhatsOnAdminPage>
  );
}
