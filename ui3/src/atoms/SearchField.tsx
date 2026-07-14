import type { ChangeEvent } from "react";
import { useEffect, useRef, useState } from "react";
import "./searchfield.css";

type SearchFieldProps = {
  value?: string;
  defaultValue?: string;
  placeholder?: string;
  onChange?: (value: string) => void;
};

export default function SearchField({
  value, defaultValue = "", placeholder = "Search", onChange,
}: SearchFieldProps) {
  const [draft, setDraft] = useState(value ?? defaultValue);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (value === undefined) return;
    if (inputRef.current && document.activeElement === inputRef.current) return;
    setDraft(value);
  }, [value]);

  function set(e: ChangeEvent<HTMLInputElement>) {
    setDraft(e.target.value);
    onChange?.(e.target.value);
  }

  return (
    <label className="search">
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" className="search__icon">
        <circle cx="7" cy="7" r="5" fill="none" stroke="currentColor" strokeWidth="1.6" />
        <path d="M11 11l3.5 3.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
      <input
        ref={inputRef}
        className="search__input" type="text" aria-label={placeholder}
        placeholder={placeholder} value={draft} onChange={set}
      />
    </label>
  );
}
