import type { CSSProperties } from "react";

import EmptyState from "../../components/EmptyState";
import "../../admin/admin.css";
import "../operator.css";

export type OpPickablePlace = {
  id: string;
  title?: string | null;
  base_position?: string | null;
  image?: string | null;
  user_count?: number | null;
  moderationHint?: string | null;
};

type OpPlacePickerProps = {
  places: OpPickablePlace[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  owner?: string | null;
  compact?: boolean;
};

const GRID: CSSProperties = { "--adm-col": "220px" } as CSSProperties;

function label(p: OpPickablePlace): string {
  return p.title || p.base_position || p.id;
}

export default function OpPlacePicker({
  places,
  selectedId,
  onSelect,
  owner,
  compact = false,
}: OpPlacePickerProps) {
  if (places.length === 0) {
    return (
      <EmptyState
        variant="inline"
        titleAs="p"
        title="No places are registered to this address."
      />
    );
  }

  if (compact) {
    return (
      <div className="adm-field">
        <label className="adm-field__label" htmlFor="op-place">
          Place
        </label>
        <select
          id="op-place"
          className="adm-input"
          value={selectedId ?? ""}
          onChange={(e) => onSelect(e.target.value)}
        >
          {selectedId == null && (
            <option value="" disabled>
              Choose a place&#x2026;
            </option>
          )}
          {places.map((p) => (
            <option key={p.id} value={p.id}>
              {label(p)}
            </option>
          ))}
        </select>
      </div>
    );
  }

  return (
    <section className="adm-stack adm-stack--lg">
      <div className="adm__head">
        <h2 className="adm__h2">Places registered to this address (public data)</h2>
        {owner && <span className="adm-mono adm-dim">{owner}</span>}
      </div>
      <div
        className="adm-grid"
        style={GRID}
        role="listbox"
        aria-label="Places registered to this address"
      >
        {places.map((p) => {
          const selected = p.id === selectedId;
          return (
            <button
              key={p.id}
              type="button"
              role="option"
              aria-selected={selected}
              className={"adm-card adm-card--link" + (selected ? " is-active" : "")}
              onClick={() => onSelect(p.id)}
            >
              {p.image ? (
                <span className="adm-thumb" style={{ backgroundImage: `url("${p.image}")` }} />
              ) : null}
              <span className="adm-card__title">{label(p)}</span>
              <span className="adm-mono adm-dim">
                {p.base_position ?? ""}
                {typeof p.user_count === "number" && p.user_count > 0
                  ? ` \u{B7} ${p.user_count} online`
                  : ""}
              </span>
              {p.moderationHint && (
                <span className="adm-status" data-tone="bad">
                  {p.moderationHint}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </section>
  );
}
