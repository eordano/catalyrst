import { useEffect, useRef } from "react";

import MkCreditsHub from "@ui/marketplace/pages/MkCreditsHub";

import {
  formatDuration,
  type CreditsHubVM,
} from "@data/lib/catalyst/marketplace/credits";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/credits";

type Props = {
  sid: string;
  hub: CreditsHubVM;
  progressUnavailable?: boolean;
  onRetryProgress?: () => void;
  onClaim: (goalTitle: string, reward: number) => void;
  onClose?: () => void;
};

export default function CreditsHub({
  sid,
  hub,
  progressUnavailable,
  onRetryProgress,
  onClaim,
  onClose,
}: Props) {
  useBalanceViewed(sid, hub);
  useGoalsViewed(sid, hub);

  useEffect(() => {
    if (!onClose) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  function handleClaim(goalTitle: string, reward: number) {
    track(
      "mk_credits_claim_clicked",
      { goal: goalTitle, reward },
      { sid, story: STORY },
    );
    onClaim(goalTitle, reward);
  }

  function handleClaimAll() {
    track(
      "mk_credits_claim_clicked",
      { goal: "__all__", reward: hub.claimable },
      { sid, story: STORY },
    );
    onClaim("__all__", hub.claimable);
  }

  return (
    <MkCreditsHub
      hub={hub}
      expiresInLabel={formatDuration(hub.expiresInSeconds)}
      timeLeftLabel={formatDuration(hub.weekSecondsRemaining)}
      progressUnavailable={progressUnavailable}
      onRetryProgress={onRetryProgress}
      onClaim={handleClaim}
      onClaimAll={handleClaimAll}
      onClose={onClose}
    />
  );
}

function useBalanceViewed(sid: string, hub: CreditsHubVM) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      "mk_credits_balance_viewed",
      {
        available: hub.available,
        claimable: hub.claimable,
        expires_in_seconds: hub.expiresInSeconds,
        blocked: hub.isBlockedForClaiming,
      },
      { sid, story: STORY },
    );
  }, [sid, hub]);
}

function useGoalsViewed(sid: string, hub: CreditsHubVM) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    const completed = hub.goals.filter(
      (g) => g.status !== "progress",
    ).length;
    track(
      "mk_credits_goal_viewed",
      { count: hub.goals.length, claimable: hub.claimable, completed },
      { sid, story: STORY },
    );
  }, [sid, hub]);
}
