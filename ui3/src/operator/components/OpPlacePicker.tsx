import "./sceneadmins.css";

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
      <p className="sa__empty">
        No places are registered to this address.
      </p>
    );
  }

  if (compact) {
    return (
      <div className="sa-picker sa-picker--compact">
        <label className="sa-picker__label" htmlFor="op-place">
          Place
        </label>
        <select
          id="op-place"
          className="sa-picker__select"
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
    <div className="op-picker">
      <div className="op-picker__head">
        <span className="op-picker__title">
          Places registered to this address (public data)
        </span>
        {owner && <span className="op-picker__owner">{owner}</span>}
      </div>
      <div
        className="op-picker__grid"
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
              className={"op-picker__card" + (selected ? " is-selected" : "")}
              onClick={() => onSelect(p.id)}
            >
              {p.image ? (
                <img className="op-picker__img" src={p.image} alt="" loading="lazy" />
              ) : null}
              <span className="op-picker__name">{label(p)}</span>
              <span className="op-picker__coords">
                {p.base_position ?? ""}
                {typeof p.user_count === "number" && p.user_count > 0
                  ? ` \u{B7} ${p.user_count} online`
                  : ""}
              </span>
              {p.moderationHint && (
                <span className="op-picker__hint">{p.moderationHint}</span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
