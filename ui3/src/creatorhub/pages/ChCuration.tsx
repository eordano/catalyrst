import { useMemo, useState } from "react";
import "./chcuration.css";
import { Caret } from "../../atoms/icons";

type CollectionType = "standard" | "third_party";
type CurationStatus =
  | "to_review"
  | "under_review"
  | "approved"
  | "rejected"
  | "disabled";
type Option = { value: string; text: string };

export type CurationCollection = {
  id: string;
  name: string;
  type: CollectionType;
  isProgrammatic?: boolean;
  status?: string | null;
  count: number;
  owner?: string | null;
  curationStatus: CurationStatus;
  assignee?: string | null;
  assigneeName?: string;
  you?: boolean;
  date: string;
  ago: string;
  createdAtMs?: number | null;
  forumLink?: string | null;
  thumbs: string[];
};

const SORT_OPTIONS: Option[] = [
  { value: "MOST_RELEVANT", text: "Most relevant" },
  { value: "CREATED_AT_DESC", text: "Newest" },
  { value: "NAME_ASC", text: "Name asc" },
  { value: "NAME_DESC", text: "Name desc" },
];

const STATUS_OPTIONS: Option[] = [
  { value: "ALL_STATUS", text: "All status" },
  { value: "under_review", text: "Under Review" },
  { value: "to_review", text: "To Review" },
  { value: "approved", text: "Approved" },
  { value: "rejected", text: "Rejected" },
];

const TYPE_OPTIONS: Option[] = [
  { value: "ALL_TYPES", text: "All types" },
  { value: "standard", text: "Standard" },
  { value: "third_party", text: "Linked" },
];

const ASSIGNEE_OPTIONS: Option[] = [
  { value: "all", text: "All assignees" },
  { value: "me", text: "Assigned to me" },
];

const TYPE_BADGE_LABEL: Record<CollectionType, string> = { standard: "Regular", third_party: "Linked Wearables" };
const STATUS_DOT_TITLE: Record<string, string> = { under_review: "Under Review", synced: "Synced", unsynced: "Unsynced", loading: "Loading..." };

const SearchGlyph = () => (
  <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
    <circle cx="7" cy="7" r="4.4" stroke="currentColor" strokeWidth="1.4" fill="none" />
    <path d="M10.4 10.4 14 14" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
  </svg>
);
const CheckGlyph = () => (
  <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
    <path d="M3 8.5 6.5 12 13 4.5" stroke="currentColor" strokeWidth="1.7" fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const CloseGlyph = () => (
  <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
    <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
  </svg>
);

function CollectionImage({ thumbs = [], count }: { thumbs?: string[]; count: number }) {
  if (count === 0 || thumbs.length === 0) {
    return (
      <div className="bdcur__image bdcur__image--empty">
        <span className="bdcur__noitems">No items</span>
      </div>
    );
  }
  const first = thumbs.slice(0, 2);
  const second = thumbs.slice(2, 4);
  const rowStyle = { height: second.length ? "50%" : "100%" };
  return (
    <div className="bdcur__image">
      {first.length > 0 && (
        <div className="bdcur__imgrow" style={rowStyle}>
          {first.map((g, i) => (
            <span key={i} className="bdcur__imgcell" style={{ background: g }} />
          ))}
        </div>
      )}
      {second.length > 0 && (
        <div className="bdcur__imgrow" style={rowStyle}>
          {second.map((g, i) => (
            <span key={i} className="bdcur__imgcell" style={{ background: g }} />
          ))}
        </div>
      )}
    </div>
  );
}

function StatusDot({ status, type }: { status?: string | null; type: CollectionType }) {
  if (!status || type === "third_party") return null;
  return <span className={"bdcur__statusdot bdcur__statusdot--" + status} title={STATUS_DOT_TITLE[status]} />;
}

function TypeBadge({ type }: { type: CollectionType }) {
  const isThirdParty = type === "third_party";
  return (
    <span className={"bdcur__badge " + (isThirdParty ? "bdcur__badge--linked" : "bdcur__badge--regular")}>
      {TYPE_BADGE_LABEL[type]}
    </span>
  );
}

function KindBadge({ type, isProgrammatic }: { type: CollectionType; isProgrammatic?: boolean }) {
  const programmatic = type === "third_party" && isProgrammatic;
  return (
    <span className={"bdcur__badge " + (programmatic ? "bdcur__badge--programmatic" : "bdcur__badge--standard")}>
      {programmatic ? "Programmatic" : "Standard"}
    </span>
  );
}

function CurationState({ status }: { status: CurationStatus }) {
  switch (status) {
    case "approved":
      return (
        <div className="bdcur__action bdcur__action--approved">
          <span className="bdcur__action-text">Approved</span> <CheckGlyph />
        </div>
      );
    case "rejected":
      return (
        <div className="bdcur__action bdcur__action--rejected">
          <span className="bdcur__action-text">Rejected</span> <CloseGlyph />
        </div>
      );
    case "disabled":
      return (
        <div className="bdcur__action bdcur__action--disabled">
          <span className="bdcur__action-text">Disabled</span> <CloseGlyph />
        </div>
      );
    case "under_review":
      return <span>Under Review</span>;
    case "to_review":
    default:
      return <span>To review</span>;
  }
}

function FilterDropdown({
  value,
  options,
  onChange,
  className,
}: {
  value: string;
  options: Option[];
  onChange: (value: string) => void;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const label = options.find((o) => o.value === value)?.text ?? "";
  return (
    <div className={"bdcur__dropdown" + (className ? " " + className : "")}>
      <button
        type="button"
        className="bdcur__dropbtn"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-haspopup="listbox"
      >
        <span className="bdcur__droplabel">{label}</span> <Caret size={11} />
      </button>
      {open && (
        <div className="bdcur__dropmenu" role="listbox">
          {options.map((o) => (
            <button
              key={o.value}
              type="button"
              role="option"
              aria-selected={o.value === value}
              className={"bdcur__dropitem" + (o.value === value ? " is-active" : "")}
              onClick={() => { onChange(o.value); setOpen(false); }}
            >
              {o.text}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function CollectionRow({ collection }: { collection: CurationCollection }) {
  const {
    name, type, isProgrammatic, status, count, owner, curationStatus,
    assignee, assigneeName, you, date, ago, forumLink, thumbs,
  } = collection;
  return (
    <tr className="bdcur__row" data-id={collection.id}>
      <td className="bdcur__cell bdcur__cell--collection">
        <div className="bdcur__imagecol">
          <CollectionImage thumbs={thumbs} count={count} />
          <div className="bdcur__info">
            <div className="bdcur__title">
              <StatusDot status={status} type={type} />
              <span className="u-truncate">{name}</span>
            </div>
            <div className="bdcur__subtitle">{count} {count === 1 ? "item" : "items"}</div>
          </div>
        </div>
      </td>
      <td className="bdcur__cell">
        <TypeBadge type={type} />
      </td>
      <td className="bdcur__cell">
        <KindBadge type={type} isProgrammatic={isProgrammatic} />
      </td>
      <td className="bdcur__cell">
        <div>{type === "third_party" ? "-" : owner}</div>
      </td>
      <td className="bdcur__cell">
        <div className="bdcur__date">
          <span>{date}</span> {ago}
        </div>
      </td>
      <td className="bdcur__cell">
        <div className="bdcur__actions bdcur__text-centered">
          <CurationState status={curationStatus} />
        </div>
      </td>
      <td className="bdcur__cell">
        <div className="bdcur__edit">
          {assignee ? (
            <>
              <div className="bdcur__curator u-truncate">
                {assigneeName}
                {you ? <> (you)</> : null}
              </div>
            </>
          ) : (
            <div className="bdcur__assignee">
              Unassigned
              <button type="button" className="bdcur__link bdcur__linkbtn">Assign to me</button>
            </div>
          )}
        </div>
      </td>
      <td className="bdcur__cell">
        <div className="bdcur__text-centered">
          {forumLink ? (
            <a className="bdcur__link" href={forumLink} target="_blank" rel="noreferrer" onClick={(e) => e.stopPropagation()}>
              Link
            </a>
          ) : (
            <span className="bdcur__muted">Not posted</span>
          )}
        </div>
      </td>
    </tr>
  );
}

export default function ChCuration({
  collections = [],
  loading = false,
  embedded: _embedded = false,
  initialStatus = "ALL_STATUS",
  initialType = "ALL_TYPES",
  initialAssignee = "all",
}: {
  collections?: CurationCollection[];
  loading?: boolean;
  embedded?: boolean;
  initialStatus?: string;
  initialType?: string;
  initialAssignee?: string;
}) {
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState("MOST_RELEVANT");
  const [filterStatus, setFilterStatus] = useState(initialStatus);
  const [filterType, setFilterType] = useState(initialType);
  const [assignee, setAssignee] = useState(initialAssignee);

  const visible = useMemo(() => {
    const q = search.trim().toLowerCase();
    const filtered = collections.filter((c) => {
      if (filterStatus !== "ALL_STATUS" && c.curationStatus !== filterStatus) return false;
      if (filterType !== "ALL_TYPES" && c.type !== filterType) return false;
      if (assignee === "me" && !c.you) return false;
      if (q) {
        const name = c.name.toLowerCase();
        const owner = (c.owner ?? "").toLowerCase();
        if (!name.includes(q) && !owner.includes(q)) return false;
      }
      return true;
    });
    const out = [...filtered];
    switch (sort) {
      case "CREATED_AT_DESC":
        out.sort((a, b) => (b.createdAtMs ?? 0) - (a.createdAtMs ?? 0));
        break;
      case "NAME_ASC":
        out.sort((a, b) => a.name.localeCompare(b.name));
        break;
      case "NAME_DESC":
        out.sort((a, b) => b.name.localeCompare(a.name));
        break;
      default:
        break;
    }
    return out;
  }, [collections, search, sort, filterStatus, filterType, assignee]);

  const total = visible.length;
  const hasResults = total > 0;
  const resultsLabel = useMemo(
    () => (total > 0 ? `${total} ${total === 1 ? "result" : "results"}` : ""),
    [total]
  );

  const body = (
      <div className="bdcur">
        <div className="bdcur__filters">
          <div className="bdcur__searchrow">
            <span className="bdcur__searchicon" aria-hidden="true"><SearchGlyph /></span>
            <input
              type="text"
              placeholder="Search by name or owner address"
              aria-label="Search by name or owner address"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>

          <div className="bdcur__controls">
            <div className="bdcur__results">{!loading && hasResults ? resultsLabel : ""}</div>
            <div className="bdcur__filtercluster">
              <FilterDropdown className="bdcur__dropdown--assignees" value={assignee} options={ASSIGNEE_OPTIONS} onChange={setAssignee} />
              <FilterDropdown value={filterType} options={TYPE_OPTIONS} onChange={setFilterType} />
              <FilterDropdown value={filterStatus} options={STATUS_OPTIONS} onChange={setFilterStatus} />
              <FilterDropdown value={sort} options={SORT_OPTIONS} onChange={setSort} />
            </div>
          </div>
        </div>

        {loading ? (
          <div className="bdcur__loader" role="status" aria-label="Loading">
            <span className="bdcur__spinner" />
          </div>
        ) : (
          <>
            {hasResults ? (
              <div className="bdcur__tablewrap">
                <table className="bdcur__table">
                  <thead>
                    <tr>
                      <th>Collection</th>
                      <th>Type</th>
                      <th>Kind</th>
                      <th>Owner</th>
                      <th>Date</th>
                      <th className="bdcur__th-centered">Status</th>
                      <th>Assignee</th>
                      <th className="bdcur__th-centered">Discussion</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visible.map((c) => (
                      <CollectionRow key={c.id} collection={c} />
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <div className="bdcur__empty">
                <div>
                  {collections.length > 0
                    ? "No collections match these filters."
                    : "There are no collections to review yet."}
                </div>
              </div>
            )}
          </>
        )}
      </div>
  );

  return body;
}
