import type { KeyboardEvent } from "react";
import { useEffect, useId, useRef, useState } from "react";
import "./dropdown.css";

type DropdownProps = {
  options?: string[];
  // Per-option tooltip text, index-aligned with options.
  optionTitles?: (string | null | undefined)[];
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  ariaLabel?: string;
};

export default function Dropdown({ options = [], optionTitles, value, defaultValue, onChange, ariaLabel }: DropdownProps) {
  const [internal, setInternal] = useState<string | undefined>(defaultValue ?? options[0]);
  const isControlled = value !== undefined;
  const cur = isControlled ? value : internal;
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const ref = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const id = useId();

  useEffect(() => {
    if (!open) return;
    function onDoc(e: MouseEvent) {
      if (ref.current && e.target instanceof Node && !ref.current.contains(e.target)) setOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  useEffect(() => {
    if (open) setActive(cur !== undefined ? Math.max(0, options.indexOf(cur)) : 0);
  }, [open]);

  function pick(opt: string) {
    if (!isControlled) setInternal(opt);
    onChange?.(opt);
    setOpen(false);
    btnRef.current?.focus();
  }

  function onKey(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key === "Escape") { if (open) { e.preventDefault(); setOpen(false); } return; }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (!open) setOpen(true);
      else if (active >= 0) {
        const sel = options[active];
        if (sel !== undefined) pick(sel);
      }
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Home" || e.key === "End") {
      e.preventDefault();
      if (!open) { setOpen(true); return; }
      if (e.key === "Home") setActive(0);
      else if (e.key === "End") setActive(options.length - 1);
      else if (e.key === "ArrowDown") setActive((a) => Math.min(options.length - 1, a + 1));
      else setActive((a) => Math.max(0, a - 1));
    }
  }

  return (
    <div className={"dropdown" + (open ? " is-open" : "")} ref={ref} onKeyDown={onKey}>
      <button
        type="button" className="dropdown__btn" ref={btnRef}
        aria-label={ariaLabel}
        aria-haspopup="listbox" aria-expanded={open}
        aria-activedescendant={open && active >= 0 ? id + "-" + active : undefined}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="dropdown__cur">{cur}</span>
        <svg viewBox="0 0 12 8" width="11" height="8" aria-hidden="true" className="dropdown__caret">
          <path d="M1 1.5L6 6.5l5-5" fill="none" stroke="currentColor" strokeWidth="1.8"
            strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open && (
        <ul className="dropdown__menu" role="listbox">
          {options.map((opt, i) => (
            <li
              key={opt} id={id + "-" + i} role="option" aria-selected={opt === cur}
              title={optionTitles?.[i] ?? undefined}
              className={
                "dropdown__opt" +
                (opt === cur ? " is-active" : "") +
                (i === active ? " is-focused" : "")
              }
              onClick={() => pick(opt)}
            >
              {opt}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
