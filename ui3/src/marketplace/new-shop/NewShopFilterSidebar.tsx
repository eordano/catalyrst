import type { ReactNode } from "react";
import { useState } from "react";
import Checkbox from "../../atoms/Checkbox";
import Toggle from "../../atoms/Toggle";
import "./newshopfiltersidebar.css";

export type FilterOption = { id: string; label: ReactNode; count?: number; checked?: boolean };
export type FilterGroup = { id: string; label: ReactNode; options: FilterOption[] };

type NewShopFilterSidebarProps = {
  groups?: FilterGroup[];
  itemCount?: ReactNode;
  onSale?: boolean;
  onToggleOnSale?: (on: boolean) => void;
  onOptionChange?: (groupId: string, optionId: string, checked: boolean) => void;
};

export default function NewShopFilterSidebar({
  groups = [],
  itemCount,
  onSale = true,
  onToggleOnSale,
  onOptionChange,
}: NewShopFilterSidebarProps) {
  const [open, setOpen] = useState(false);
  return (
    <aside className="nsfilter" aria-label="Filters" data-open={open}>
      <button
        type="button"
        className="nsfilter__toggle"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span>Filters</span>
        <span className="nsfilter__toggle-caret" aria-hidden="true">
          {open ? "\u{25B2}" : "\u{25BC}"}
        </span>
      </button>

      <div className="nsfilter__panel">
      {itemCount != null ? <div className="nsfilter__count">{itemCount}</div> : null}

      {onToggleOnSale ? (
        <div className="nsfilter__onsale">
          <span className="nsfilter__onsale-label">On Sale</span>
          <Toggle checked={onSale} onChange={onToggleOnSale} ariaLabel="On Sale" />
        </div>
      ) : null}

      {groups.map((g) => (
        <div
          className="nsfilter__group"
          key={g.id}
          role="group"
          aria-labelledby={"nsfilter-h-" + g.id}
        >
          <div className="nsfilter__group-head" id={"nsfilter-h-" + g.id}>
            {g.label}
          </div>
          <div className="nsfilter__options">
            {g.options.map((o) => (
              <div className="nsfilter__opt" key={o.id}>
                <Checkbox
                  checked={o.checked}
                  defaultChecked={o.checked === undefined ? false : undefined}
                  onChange={(c) => onOptionChange?.(g.id, o.id, c)}
                >
                  {o.label}
                </Checkbox>
                {o.count != null ? (
                  <span className="nsfilter__opt-count">{o.count}</span>
                ) : null}
              </div>
            ))}
          </div>
        </div>
      ))}
      </div>
    </aside>
  );
}
