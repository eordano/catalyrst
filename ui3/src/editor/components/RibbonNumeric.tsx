import { useEffect, useRef, useState } from "react";
import { tidy } from "../transform-nudge";

export type NumericField = "position" | "rotation";
export type NumericAxis = "x" | "y" | "z";

export interface RibbonNumericValue {
  x: number;
  y: number;
  z: number;
}

export interface RibbonNumericProps {
  position?: RibbonNumericValue | null;
  rotation?: RibbonNumericValue | null;
  /** Absolute setter. Absent means the capability is missing and the group is omitted. */
  onCommit: (field: NumericField, axis: NumericAxis, value: number) => void;
  /** Arrow-key increment for the three position fields. */
  step?: number;
  /** Arrow-key increment for the rotation field. */
  angleStep?: number;
}

function display(n: number | undefined): string {
  if (n === undefined || !Number.isFinite(n)) return "";
  return String(Math.round(n * 1e3) / 1e3);
}

export interface NumFieldProps {
  label: string;
  ariaLabel: string;
  value: number | undefined;
  step: number;
  onCommit: (value: number) => void;
}

// The prototype committed parseFloat on every keystroke, so a minus sign, a
// decimal point and an empty field were all impossible to type. The draft
// string is what makes those states legal until the value is actually applied.
export function NumField({ label, ariaLabel, value, step, onCommit }: NumFieldProps) {
  const [draft, setDraft] = useState(() => display(value));
  const [editing, setEditing] = useState(false);
  const ref = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!editing) setDraft(display(value));
  }, [value, editing]);

  // A blur that changed nothing must not write to the scene, or every tab
  // through the row would queue four redundant Transform writes.
  const commit = (raw: string) => {
    const n = parseFloat(raw);
    if (!Number.isFinite(n) || (value !== undefined && Math.abs(n - value) < 1e-9)) {
      setDraft(display(value));
      return;
    }
    onCommit(n);
    setDraft(display(n));
  };

  return (
    <span className="rb-num">
      <span className="rb-num-ax" aria-hidden="true">
        {label}
      </span>
      <input
        ref={ref}
        className="rb-num-in"
        type="text"
        inputMode="decimal"
        spellCheck={false}
        aria-label={ariaLabel}
        value={draft}
        onFocus={() => setEditing(true)}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          setEditing(false);
          commit(draft);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit(draft);
            return;
          }
          if (e.key === "Escape") {
            e.preventDefault();
            setDraft(display(value));
            setEditing(false);
            ref.current?.blur();
            return;
          }
          if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
          e.preventDefault();
          const base = Number.isFinite(parseFloat(draft)) ? parseFloat(draft) : (value ?? 0);
          const mult = e.shiftKey ? 10 : 1;
          const next = base + (e.key === "ArrowUp" ? step : -step) * mult;
          const tidied = tidy(next);
          setDraft(display(tidied));
          onCommit(tidied);
        }}
      />
    </span>
  );
}

export default function RibbonNumeric({
  position = null,
  rotation = null,
  onCommit,
  step = 0.25,
  angleStep = 15,
}: RibbonNumericProps) {
  return (
    <div className="rb-numeric">
      {(["x", "y", "z"] as const).map((ax) => (
        <NumField
          key={ax}
          label={ax.toUpperCase()}
          ariaLabel={`Position ${ax.toUpperCase()}`}
          value={position?.[ax]}
          step={step}
          onCommit={(v) => onCommit("position", ax, v)}
        />
      ))}
      <NumField
        label={"R\u{00B0}"}
        ariaLabel="Rotation Y"
        value={rotation?.y}
        step={angleStep}
        onCommit={(v) => onCommit("rotation", "y", v)}
      />
    </div>
  );
}
