import type { ReactNode } from "react";
import "./filterradios.css";

type FilterRadioOption = { id: string; label: ReactNode };

type FilterRadiosProps = {
  name?: string;
  value?: string;
  onChange?: (id: string) => void;
  options?: FilterRadioOption[];
};

export default function FilterRadios({ name, value, onChange, options = [] }: FilterRadiosProps) {
  return (
    <div className="filter-radios">
      {options.map((o) => (
        <label key={o.id} className="filter-radios__radio">
          <input
            type="radio"
            name={name}
            checked={value === o.id}
            onChange={() => onChange?.(o.id)}
          />
          <span className="filter-radios__mark" aria-hidden="true" />
          {o.label}
        </label>
      ))}
    </div>
  );
}
