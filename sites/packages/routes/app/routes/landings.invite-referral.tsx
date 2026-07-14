import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router";

import LdInviteReferralView from "@ui/landings/pages/LdInviteReferralView";

import { loadInviteReferral } from "@data/lib/catalyst/landings/referral.server";
import type { ReferrerVM, ReferralState } from "@data/lib/catalyst/landings/referral";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/landings.invite-referral";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/invite-referral";

type Faq = { q: string; a: string };
type Step = "view" | "download" | "rewards" | "faqs";

function parseStep(raw: string | null): Step {
  switch (raw) {
    case "download":
    case "rewards":
    case "faqs":
      return raw;
    default:
      return "view";
  }
}

const FALLBACK: Assignment = {
  variant: "personalized_referrer",
  flags: { showReferrerName: true, resolveProfile: true },
  experimentKey: "lp_invite_referral",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const referrerParam = url.searchParams.get("referrer")?.trim() ?? "";
  const step = parseStep(url.searchParams.get("step"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const load = await loadInviteReferral(referrerParam, request.signal).catch(
    () => null,
  );

  const attribution =
    load && load.referrerParam
      ? {
          simulated: true as const,
          referrer: load.referrer.address ?? load.referrer.name,
          session: sid,
        }
      : null;

  const payload = {
    sid,
    step,
    referrerParam,
    referrer: load?.referrer ?? null,
    referral: load?.referral ?? null,
    faqs: (load?.faqs ?? []) as Faq[],
    isFixture: load?.isFixture ?? true,
    attribution,
  };

  return wrap(payload);
}

export default function InviteReferral({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return (
    <InviteLanding
      sid={d.sid}
      step={d.step}
      referrerParam={d.referrerParam}
      referrer={d.referrer}
      referral={d.referral}
    />
  );
}

type LandingProps = {
  sid: string;
  step: Step;
  referrerParam: string;
  referrer: ReferrerVM | null;
  referral: ReferralState | null;
};

function InviteLanding({ sid, step, referrerParam, referrer, referral }: LandingProps) {
  const [, setSearchParams] = useSearchParams();

  const hasReferrer = Boolean(referrerParam);
  const handle = referrer?.name ?? "A friend";

  useInviteTelemetry(sid, step, hasReferrer, referrer, referral);

  function updateParam(key: string, value: string) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (value) next.set(key, value);
        else next.delete(key);
        return next;
      },
      { preventScrollReset: false },
    );
  }

  return (
    <LdInviteReferralView
      step={step}
      handle={handle}
      referrerAddress={referrer?.address ?? null}
      referrerHasClaimedName={referrer?.hasClaimedName ?? false}
      invitedUsersAccepted={referral?.invitedUsersAccepted ?? 0}
      onStep={(s) => updateParam("step", s === "view" ? "" : s)}
    />
  );
}

function useInviteTelemetry(
  sid: string,
  step: Step,
  hasReferrer: boolean,
  referrer: ReferrerVM | null,
  referral: ReferralState | null,
) {
  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "lp_invite_viewed",
      {
        has_referrer: hasReferrer,
        resolved: referrer?.resolved ?? false,
        referrer_handle: referrer?.name ?? null,
      },
      { sid, story: STORY },
    );
  }, [sid, hasReferrer, referrer]);

  const resolvedFired = useRef(false);
  useEffect(() => {
    if (resolvedFired.current || !hasReferrer || !referrer) return;
    resolvedFired.current = true;
    track(
      "lp_invite_referrer_resolved",
      {
        referrer_handle: referrer.name,
        resolved: referrer.resolved,
        source: referrer.resolved ? "profile_lambdas" : "param",
      },
      { sid, story: STORY },
    );
  }, [sid, hasReferrer, referrer]);

  const rewardsFired = useRef(false);
  useEffect(() => {
    if (step !== "rewards" || rewardsFired.current || !referral) return;
    rewardsFired.current = true;
    track(
      "lp_invite_rewards_viewed",
      {
        accepted: referral.invitedUsersAccepted,
        current_tier: referral.currentTier,
      },
      { sid, story: STORY },
    );
  }, [sid, step, referral]);
}
