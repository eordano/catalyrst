import { Avatar } from "../../atoms/primitives";
import {
  STATUS_TONE,
  truncateAddress,
  type CommunityModerationCard,
} from "./AdCommunityTypes";

type CommunityReviewCardProps = {
  card: CommunityModerationCard;
};

export default function AdCommunityReviewCard({ card }: CommunityReviewCardProps) {
  return (
    <div className="adm-card" role="region" aria-label={`Review ${card.name}`}>
      <div className="adm-card__head">
        <Avatar
          hue={card.hue}
          size={56}
          src={card.thumbnail || undefined}
          name={card.name}
        />
        <div>
          <h2 className="adm-card__title">
            {card.name}{" "}
            <span className="adm-status" data-tone={STATUS_TONE[card.status]}>
              {card.status}
            </span>
          </h2>
          <span className="adm-dim">
            owned by <code>{truncateAddress(card.owner)}</code>
            {card.ownerName ? ` (${card.ownerName})` : ""}
          </span>
        </div>
      </div>

      <dl className="adm-stats">
        <div>
          <dt>Privacy</dt>
          <dd>{card.privacy}</dd>
        </div>
        <div>
          <dt>Members</dt>
          <dd>{card.membersCount.toLocaleString()}</dd>
        </div>
        <div>
          <dt>Active</dt>
          <dd>{card.active ? "yes" : "no"}</dd>
        </div>
      </dl>

      {card.flaggedReason ? (
        <div className="adm-notice" data-tone="warn" role="note">
          <p>
            <span aria-hidden="true">&#x2691; </span>
            <strong>Flagged: </strong>
            {card.flaggedReason}
          </p>
        </div>
      ) : (
        <p className="adm-card__text adm-dim">No active flags on this community.</p>
      )}
    </div>
  );
}
