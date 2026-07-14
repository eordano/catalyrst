import type { ReactNode } from "react";
import SearchField from "../../atoms/SearchField";
import "./assettopbar.css";

const GridGlyph = (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <rect x="3" y="3" width="7" height="7" rx="1.5" />
    <rect x="14" y="3" width="7" height="7" rx="1.5" />
    <rect x="3" y="14" width="7" height="7" rx="1.5" />
    <rect x="14" y="14" width="7" height="7" rx="1.5" />
  </svg>
);
const ListGlyph = (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
    <path d="M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01" />
  </svg>
);

type ViewOption = { id: string; label: string; icon: ReactNode };
type SortOption = { id: string; label: ReactNode };

const DEFAULT_VIEWS: ViewOption[] = [
  { id: "grid", label: "Grid view", icon: GridGlyph },
  { id: "list", label: "List view", icon: ListGlyph },
];

type ViewToggleProps = {
  value?: string;
  onChange?: (id: string) => void;
  options: ViewOption[];
};

function ViewToggle({ value, onChange, options }: ViewToggleProps) {
  return (
    <div className="topbar__view" role="group" aria-label="View mode">
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          className={"topbar__viewbtn" + (value === o.id ? " is-active" : "")}
          aria-pressed={value === o.id}
          aria-label={o.label}
          onClick={() => onChange && onChange(o.id)}
        >
          {o.icon}
        </button>
      ))}
    </div>
  );
}

type SortSelectProps = {
  value?: string;
  onChange?: (value: string) => void;
  options: SortOption[];
  label?: string;
};

function SortSelect({ value, onChange, options, label }: SortSelectProps) {
  return (
    <label className="topbar__sort">
      {label ? <span className="topbar__sortlabel">{label}</span> : null}
      <span className="topbar__sortwrap">
        <select
          className="topbar__sortselect"
          value={value}
          onChange={(e) => onChange && onChange(e.target.value)}
          aria-label={label || "Sort by"}
        >
          {options.map((s) => (
            <option key={s.id} value={s.id}>{s.label}</option>
          ))}
        </select>
        <svg className="topbar__sortcaret" viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
          <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" fill="none" />
        </svg>
      </span>
    </label>
  );
}

type AssetTopbarProps = {
  layout?: "inline" | "stacked";
  searchPlaceholder?: string;
  search?: string;
  onSearch?: (value: string) => void;
  count?: ReactNode;
  countNoun?: [string, string];
  showCount?: boolean;
  sort?: string;
  onSort?: (value: string) => void;
  sortOptions?: SortOption[];
  sortLabel?: string;
  view?: string;
  onView?: (id: string) => void;
  viewOptions?: ViewOption[];
  className?: string;
};

export default function AssetTopbar({
  layout = "inline",
  searchPlaceholder = "Search",
  search,
  onSearch,
  count,
  countNoun = ["item", "items"],
  showCount = true,
  sort,
  onSort,
  sortOptions,
  sortLabel = "",
  view,
  onView,
  viewOptions,
  className = "",
}: AssetTopbarProps) {
  const hasSort = Array.isArray(sortOptions) && sortOptions.length > 0;
  const views: ViewOption[] | null = viewOptions || (view !== undefined ? DEFAULT_VIEWS : null);
  const hasView = Array.isArray(views) && views.length > 0;
  const countText =
    typeof count === "number"
      ? `${count.toLocaleString()} ${count === 1 ? countNoun[0] : countNoun[1]}`
      : count;

  const searchEl = (
    <SearchField placeholder={searchPlaceholder} value={search} onChange={onSearch} />
  );
  const sortEl = hasSort ? (
    <SortSelect value={sort} onChange={onSort} options={sortOptions ?? []} label={sortLabel} />
  ) : null;
  const viewEl = hasView ? <ViewToggle value={view} onChange={onView} options={views ?? []} /> : null;

  if (layout === "stacked") {
    return (
      <div className={"topbar topbar--stacked" + (className ? " " + className : "")}>
        <div className="topbar__searchrow">
          <div className="topbar__search">{searchEl}</div>
          {viewEl}
        </div>
        {(showCount || hasSort) ? (
          <div className="topbar__inforow">
            {showCount ? <p className="topbar__count" role="status">{countText}</p> : <span />}
            {sortEl}
          </div>
        ) : null}
      </div>
    );
  }

  return (
    <div className={"topbar topbar--inline" + (className ? " " + className : "")}>
      <div className="topbar__search">{searchEl}</div>
      {showCount ? <div className="topbar__count topbar__count--inline" role="status">{countText}</div> : null}
      <div className="topbar__tools">
        {sortEl}
        {viewEl}
      </div>
    </div>
  );
}
