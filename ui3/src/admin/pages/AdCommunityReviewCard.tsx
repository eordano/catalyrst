import { Avatar } from "../../atoms/primitives";
import {
  STATUS_CLASS,
  truncateAddress,
  type CommunityModerationCard,
} from "./AdCommunityTypes";

export type CommunityReviewCardProps = {
  card: CommunityModerationCard;
};

export default function AdCommunityReviewCard({ card }: CommunityReviewCardProps) {
  return (
    <div className="crc" role="region" aria-label={`Review ${card.name}`}>
      <div className="crc__head">
        <Avatar hue={card.hue} size={56} className="crc__avatar" />
        <div className="crc__headtext">
          <h2 className="crc__name">{card.name}</h2>
          <span className="crc__owner">
            owned by <code>{truncateAddress(card.owner)}</code>
            {card.ownerName ? ` (${card.ownerName})` : ""}
          </span>
        </div>
        <span className={STATUS_CLASS[card.status]}>{card.status}</span>
      </div>

      <dl className="crc__stats">
        <div className="crc__stat">
          <dt>Privacy</dt>
          <dd>{card.privacy}</dd>
        </div>
        <div className="crc__stat">
          <dt>Members</dt>
          <dd>{card.membersCount.toLocaleString()}</dd>
        </div>
        <div className="crc__stat">
          <dt>Active</dt>
          <dd>{card.active ? "yes" : "no"}</dd>
        </div>
      </dl>

      {card.flaggedReason ? (
        <div className="crc__flag" role="note">
          <span className="crc__flagicon" aria-hidden="true">
            &#x2691;
          </span>
          <span>
            <strong>Flagged: </strong>
            {card.flaggedReason}
          </span>
        </div>
      ) : (
        <p className="crc__noflag">No active flags on this community.</p>
      )}
    </div>
  );
}
