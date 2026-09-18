import type { ReactNode } from "react";

import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import AdCommunityModerationList from "./AdCommunityModerationList";
import AdCommunityReviewCard from "./AdCommunityReviewCard";
import AdSuspendDecisionBar from "./AdSuspendDecisionBar";
import type {
  CommunityDecision,
  CommunityModerationCard,
  CommunityStatus,
  ModerateCommunitiesStateValue,
} from "./AdCommunityTypes";
import "../admin.css";

type AdCommunitiesModerationPageProps = {
  nav?: ReactNode;
  step: string;
  value: ModerateCommunitiesStateValue;
  cards: CommunityModerationCard[];
  search: string;
  status: CommunityStatus;
  counts: Record<CommunityStatus, number>;
  activeCard?: CommunityModerationCard;
  decision: CommunityDecision;
  error?: string;
  resultSuspended?: boolean;
  onSignIn: () => void;
  onSearch: (value: string) => void;
  onStatus: (status: CommunityStatus) => void;
  onReview: (communityId: string) => void;
  onBack: () => void;
  onDecide: (decision: CommunityDecision, reason?: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
  onContinue: () => void;
};

export default function AdCommunitiesModerationPage({
  nav = undefined,
  step,
  value,
  cards,
  search,
  status,
  counts,
  activeCard = undefined,
  decision,
  error = undefined,
  resultSuspended = undefined,
  onSignIn,
  onSearch,
  onStatus,
  onReview,
  onBack,
  onDecide,
  onConfirm,
  onCancel,
  onContinue,
}: AdCommunitiesModerationPageProps) {
  return (
    <main className="adm" data-step={step} data-state={value}>
      {nav ? (
        <nav className="adm__nav" aria-label="Admin consoles">
          {nav}
        </nav>
      ) : null}
      <div className="adm__page">
        {value === "authGate" && (
          <div
            className="adm-gate"
            role="region"
            aria-label="Community moderation notice"
          >
            <div className="adm-card">
              <h2 className="adm-card__title">Community moderation</h2>
              <p className="adm-card__text">
                Browsing the community list is public and unauthenticated
                (<code>communities.rs:176</code>, <code>try_extract_signer</code>,
                optional). <strong>Suspend / unsuspend performs a real moderation
                write</strong> held server-side: the browser posts to
                <code>/admin/community-suspension</code>, whose action holds this
                node&apos;s admin bearer. The server-side check is
                <code>require_admin</code> in the communities service, which
                answers 403 &ldquo;admin controls disabled (API_ADMIN_TOKEN
                unset)&rdquo; when no token is configured. This panel authorizes
                nothing &#x2014; it is a notice, and continuing past it grants no
                access.
              </p>
              <div className="adm-actions adm-actions--start">
                <Button onClick={onSignIn}>Continue to moderation list</Button>
              </div>
            </div>
          </div>
        )}

        {value === "list" && (
          <AdCommunityModerationList
            cards={cards}
            search={search}
            status={status}
            counts={counts}
            onSearch={onSearch}
            onStatus={onStatus}
            onReview={onReview}
          />
        )}

        {(value === "reviewCommunity" || value === "decision") && activeCard && (
          <div className="adm__inner adm__inner--mid">
            <button type="button" className="adm-back" onClick={onBack}>
              &#x2190; Back to list
            </button>
            <AdCommunityReviewCard card={activeCard} />

            {value === "reviewCommunity" && (
              <div className="adm-actions">
                <Button
                  variant="secondary"
                  tone="success"
                  onClick={() => onDecide("unsuspend")}
                  disabled={activeCard.suspended !== true}
                  title={
                    activeCard.suspended === null
                      ? "Suspension state was not reported for this community"
                      : activeCard.suspended
                        ? ""
                        : "Community is not suspended"
                  }
                >
                  Unsuspend
                </Button>
                <Button
                  variant="primary"
                  tone="danger"
                  onClick={() => onDecide("suspend")}
                  disabled={activeCard.suspended === true}
                  title={activeCard.suspended ? "Community is already suspended" : ""}
                >
                  Suspend&#x2026;
                </Button>
              </div>
            )}

            {value === "decision" && (
              <AdSuspendDecisionBar
                suspended={activeCard.suspended}
                decision={decision}
                error={error}
                onDecide={onDecide}
                onConfirm={onConfirm}
                onCancel={onCancel}
              />
            )}
          </div>
        )}

        {value === "submitting" && (
          <div className="adm-gate">
            <div className="adm-card" role="status">
              <div className="adm-actions adm-actions--start">
                <Spinner size={22} aria-hidden="true" />
                <span className="adm-card__text">Applying moderation&#x2026;</span>
              </div>
            </div>
          </div>
        )}

        {value === "moderated" && activeCard && (
          <div className="adm-gate">
            <div className="adm-card" role="status" aria-live="polite">
              <h2 className="adm-card__title">Moderation applied</h2>
              <p className="adm-card__text">
                <strong>{activeCard.name}</strong> &middot;{" "}
                {resultSuspended ? "Suspended" : "Unsuspended"}.
              </p>
              <div className="adm-actions adm-actions--start">
                <Button onClick={onContinue}>Back to list</Button>
              </div>
            </div>
          </div>
        )}
      </div>
    </main>
  );
}
