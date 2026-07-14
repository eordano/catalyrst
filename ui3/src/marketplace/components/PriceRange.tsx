import "./pricerange.css";

const BARS = [6, 12, 20, 33, 28, 46, 60, 78, 95, 74, 66, 82, 54, 47, 61, 42, 31, 38, 25, 19, 28, 15, 10, 7];

type PriceRangeProps = {
  minPlaceholder?: string;
  maxPlaceholder?: string;
  chart?: boolean;
  className?: string;
};

export default function PriceRange({ minPlaceholder = "0", maxPlaceholder = "∞", chart = true, className = "" }: PriceRangeProps) {
  return (
    <div className={"pricerange" + (className ? " " + className : "")}>
      <div className="pricerange__row">
        <label className="pricerange__field">
          <span className="pricerange__label">Min</span>
          <input
            type="text"
            inputMode="numeric"
            className="pricerange__input"
            placeholder={minPlaceholder}
            aria-label="Minimum price"
          />
        </label>
        <span className="pricerange__dash" aria-hidden="true">—</span>
        <label className="pricerange__field">
          <span className="pricerange__label">Max</span>
          <input
            type="text"
            inputMode="numeric"
            className="pricerange__input"
            placeholder={maxPlaceholder}
            aria-label="Maximum price"
          />
        </label>
      </div>
      {chart ? (
        <div className="pricerange__chart" aria-hidden="true">
          {BARS.map((h, i) => (
            <span key={i} className="pricerange__bar" style={{ height: h + "%" }} />
          ))}
        </div>
      ) : null}
    </div>
  );
}
