import StInvite from "../../web/pages/StInvite";
import StProfileReferralRewardsTab from "../../web/pages/StProfileReferralRewardsTab";
import "./ldinvitereferralview.css";

export type LdInviteStep = "view" | "download" | "rewards" | "faqs";

type LdInviteReferralViewProps = {
  step?: LdInviteStep;
  handle?: string;
  referrerAddress?: string | null;
  referrerHasClaimedName?: boolean;
  invitedUsersAccepted?: number;
  onStep?: (step: LdInviteStep) => void;
};

export default function LdInviteReferralView({
  step = "view",
  handle = "A friend",
  referrerAddress = null,
  referrerHasClaimedName = false,
  invitedUsersAccepted = 0,
  onStep = undefined,
}: LdInviteReferralViewProps) {
  if (step === "rewards") {
    return (
      <main className="invite-referral-route" data-step="rewards">
        <LdInviteStepNav step={step} onStep={onStep} />
        <StProfileReferralRewardsTab
          profile={{
            address: referrerAddress ?? "0x0000000000000000000000000000000000000000",
            name: handle,
            hasClaimedName: referrerHasClaimedName,
            nameColor: "#FF8362",
          }}
          activeTab="referral-rewards"
          data={{
            invitedUsersAccepted,
            invitedUsersAcceptedViewed: invitedUsersAccepted,
            rewardImages: [],
          }}
          state="ready"
        />
      </main>
    );
  }

  return (
    <main className="invite-referral-route">
      <StInvite referrer={{ name: handle, ethAddress: referrerAddress ?? "" }} loading={false} />
    </main>
  );
}

function LdInviteStepNav({
  step,
  onStep,
}: {
  step: LdInviteStep;
  onStep?: (s: LdInviteStep) => void;
}) {
  const steps: LdInviteStep[] = ["view", "download", "rewards", "faqs"];
  return (
    <nav className="invite-referral-route__steps" aria-label="Journey steps">
      {steps.map((s) => (
        <button
          key={s}
          type="button"
          className={"invite-referral-route__step" + (s === step ? " is-active" : "")}
          aria-current={s === step ? "step" : undefined}
          onClick={() => onStep?.(s)}
        >
          {s}
        </button>
      ))}
    </nav>
  );
}
