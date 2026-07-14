import { useMemo } from "react";
import { Link } from "react-router";

import LdEventSubscriptionsView from "@ui/landings/pages/LdEventSubscriptionsView";

import {
  fetchProfileSettings,
  toSubscriptionSettingsView,
  buildSubscriptionCommit,
  NOTIFICATION_GROUPS,
} from "@data/lib/catalyst/landings/subscriptions";
import { useAuth } from "@data/lib/auth/context";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import SubscriptionWizard, {
  type SubscriptionGroup,
} from "@features/stories/landings/event-subscriptions/SubscriptionWizard";

import type { Route } from "./+types/landings.event-subscriptions";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/event-subscriptions";

function coarseSelection(
  groups: SubscriptionGroup[],
  on: boolean,
): Record<string, boolean> {
  const out: Record<string, boolean> = {};
  for (const group of groups) {
    for (const type of group.types) out[type] = on;
  }
  return out;
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "landings_event_subscriptions",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const live = await fetchProfileSettings({ signal: request.signal });
  const usedLive = live !== null;

  const groups = NOTIFICATION_GROUPS as SubscriptionGroup[];
  const settings = usedLive
    ? toSubscriptionSettingsView(live)
    : { email: "", emailConfirmed: false, subscribed: false };

  const view = {
    email: settings.email,
    emailConfirmed: settings.emailConfirmed,
    subscribed: settings.subscribed,
    selection: coarseSelection(groups, settings.subscribed),
    groups,
  };

  const fallback = !usedLive;

  const payload = { sid, step, assignment, view, usedLive, fallback };
  return wrap(payload);
}

export default function LandingsEventSubscriptions({
  loaderData,
}: Route.ComponentProps) {
  const { sid, step, assignment, view, usedLive } = loaderData;

  const { identity } = useAuth();
  const commit = useMemo(() => buildSubscriptionCommit(identity), [identity]);

  return (
    <LdEventSubscriptionsView signedOut={!usedLive} LinkComponent={Link}>
      <SubscriptionWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        email={view.email}
        emailConfirmed={view.emailConfirmed}
        selection={view.selection}
        groups={view.groups}
        initialStep={step ?? undefined}
        commit={commit}
      />
    </LdEventSubscriptionsView>
  );
}
