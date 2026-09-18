import { Avatar } from "../../atoms/primitives";
import Button from "../../atoms/Button";
import SearchField from "../../atoms/SearchField";
import {
  COMMUNITY_STATUSES,
  STATUS_TONE,
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

type CommunityModerationListProps = {
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
    <div className="adm__inner">
      <div className="adm__head">
        <h1 className="adm__title">Communities moderation</h1>
      </div>

      <div className="adm__tools">
        <SearchField
          placeholder="Search by community name"
          value={search}
          onChange={onSearch}
        />
        <div className="adm-pills" role="tablist" aria-label="Status filter">
          {COMMUNITY_STATUSES.map((s) => (
            <button
              key={s}
              type="button"
              role="tab"
              aria-selected={status === s}
              className={"adm-pill" + (status === s ? " is-active" : "")}
              onClick={() => onStatus(s)}
            >
              {STATUS_LABEL[s]}
              <span className="adm-pill__count">{counts[s]}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="adm-scroll">
        <table className="adm-table" aria-label="Communities">
          <thead>
            <tr>
              <th>Community</th>
              <th>Owner</th>
              <th className="is-center">Privacy</th>
              <th className="is-num">Members</th>
              <th className="is-center">Status</th>
              <th className="is-center">Flagged</th>
              <th className="is-center" aria-label="Action" />
            </tr>
          </thead>
          <tbody>
            {cards.map((card) => (
              <tr key={card.id} className="is-link" onClick={() => onReview(card.id)}>
                <td className="is-nowrap">
                  <Avatar
                    hue={card.hue}
                    size={36}
                    src={card.thumbnail || undefined}
                    name={card.name}
                  />
                  <strong>{card.name}</strong>
                </td>
                <td>
                  <span className="adm-mono">{truncateAddress(card.owner)}</span>
                  {card.ownerName ? (
                    <span className="adm-dim">{` (${card.ownerName})`}</span>
                  ) : null}
                </td>
                <td className="is-center">{card.privacy}</td>
                <td className="is-num">{card.membersCount.toLocaleString()}</td>
                <td className="is-center">
                  <StatusPill status={card.status} />
                </td>
                <td className="is-center">
                  {card.flaggedReason ? (
                    <span className="adm-warn" aria-label="Flagged" title={card.flaggedReason}>
                      &#x2691;
                    </span>
                  ) : null}
                </td>
                <td className="is-center">
                  <Button
                    variant="secondary"
                    size="sm"
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
                <td className="is-empty" colSpan={7}>
                  No communities match this filter.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function StatusPill({ status }: { status: CommunityModerationCard["status"] }) {
  return (
    <span className="adm-status" data-tone={STATUS_TONE[status]}>
      {status}
    </span>
  );
}
