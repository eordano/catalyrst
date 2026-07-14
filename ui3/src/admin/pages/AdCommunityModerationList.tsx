import { Avatar } from "../../atoms/primitives";
import Button from "../../atoms/Button";
import {
  COMMUNITY_STATUSES,
  STATUS_CLASS,
  truncateAddress,
  type CommunityModerationCard,
  type CommunityStatus,
} from "./AdCommunityTypes";

const STATUS_LABEL: Record<CommunityStatus, string> = {
  all: "All",
  active: "Active",
  suspended: "Suspended",
  inactive: "Inactive",
};

export type CommunityModerationListProps = {
  cards: CommunityModerationCard[];
  search: string;
  status: CommunityStatus;
  counts: Record<CommunityStatus, number>;
  onSearch: (value: string) => void;
  onStatus: (status: CommunityStatus) => void;
  onReview: (communityId: string) => void;
};

export default function AdCommunityModerationList({
  cards,
  search,
  status,
  counts,
  onSearch,
  onStatus,
  onReview,
}: CommunityModerationListProps) {
  return (
    <div className="au">
      <div className="au__container">
        <h1 className="au__title">Communities moderation</h1>

        <div className="au__header">
          <div className="au__searchwrap">
            <span className="au__searchicon" aria-hidden="true">
              <SearchIcon />
            </span>
            <input
              className="au__search"
              placeholder="Search by community name"
              aria-label="Search communities"
              value={search}
              onChange={(e) => onSearch(e.target.value)}
            />
            <span className="au__searchlabel">Search</span>
          </div>

          <div className="cml-pills" role="tablist" aria-label="Status filter">
            {COMMUNITY_STATUSES.map((s) => (
              <button
                key={s}
                type="button"
                role="tab"
                aria-selected={status === s}
                className={"cml-pill" + (status === s ? " is-active" : "")}
                onClick={() => onStatus(s)}
              >
                {STATUS_LABEL[s]}
                <span className="cml-pill__count">{counts[s]}</span>
              </button>
            ))}
          </div>
        </div>

        <div className="au__tablewrap">
          <table className="au__table" aria-label="Communities">
            <thead>
              <tr>
                <th className="au__th">Community</th>
                <th className="au__th">Owner</th>
                <th className="au__th au__th--center">Privacy</th>
                <th className="au__th au__th--center">Members</th>
                <th className="au__th au__th--center">Status</th>
                <th className="au__th au__th--center">Flagged</th>
                <th className="au__th au__th--center" aria-label="Action" />
              </tr>
            </thead>
            <tbody>
              {cards.map((card) => (
                <tr key={card.id} className="au-row" onClick={() => onReview(card.id)}>
                  <td className="au-cell au-cell--user">
                    <Avatar hue={card.hue} size={36} className="cml-avatar" />
                    <span className="cml-name">{card.name}</span>
                  </td>
                  <td className="au-cell">
                    <span className="au-cell__addr">{truncateAddress(card.owner)}</span>
                    {card.ownerName ? (
                      <span className="au-cell__name">{` (${card.ownerName})`}</span>
                    ) : null}
                  </td>
                  <td className="au-cell au-cell--center">{card.privacy}</td>
                  <td className="au-cell au-cell--center">{card.membersCount.toLocaleString()}</td>
                  <td className="au-cell au-cell--center">
                    <StatusPill status={card.status} />
                  </td>
                  <td className="au-cell au-cell--center">
                    {card.flaggedReason ? (
                      <span className="cml-flag" aria-label="Flagged" title={card.flaggedReason}>
                        &#x2691;
                      </span>
                    ) : null}
                  </td>
                  <td className="au-cell au-cell--center">
                    <Button
                      variant="secondary"
                      size="sm"
                      className="cml-reviewbtn"
                      onClick={(e) => {
                        e.stopPropagation();
                        onReview(card.id);
                      }}
                    >
                      Review
                    </Button>
                  </td>
                </tr>
              ))}
              {cards.length === 0 && (
                <tr>
                  <td className="au-cell au-cell--center au-cell--empty" colSpan={7}>
                    No communities match this filter.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function StatusPill({ status }: { status: CommunityModerationCard["status"] }) {
  return <span className={STATUS_CLASS[status]}>{status}</span>;
}

function SearchIcon() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
      <path d="M15.5 14h-.79l-.28-.27a6.5 6.5 0 1 0-.7.7l.27.28v.79l5 4.99L20.49 19l-4.99-5Zm-6 0A4.5 4.5 0 1 1 14 9.5 4.5 4.5 0 0 1 9.5 14Z" />
    </svg>
  );
}
