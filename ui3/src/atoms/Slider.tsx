import type { ChangeEvent, CSSProperties } from "react";
import { useState } from "react";
import "./slider.css";

type SliderProps = {
  value?: number;
  defaultValue?: number;
  min?: number;
  max?: number;
  step?: number;
  onChange?: (value: number) => void;
  // Commit-on-release: drag updates render locally (and still fire onChange if
  // given), onCommit fires once when the pointer or key is released.
  onCommit?: (value: number) => void;
  format?: (value: number) => number | string;
  label?: string;
  ariaLabel?: string;
  disabled?: boolean;
};

export default function Slider({
  value,
  defaultValue = 50,
  min = 0,
  max = 100,
  step = 1,
  onChange,
  onCommit,
  format = (v) => Math.round(v),
  label,
  ariaLabel,
  disabled = false,
}: SliderProps) {
  const [internal, setInternal] = useState(defaultValue);
  const [drag, setDrag] = useState<number | null>(null);
  const isControlled = value !== undefined;
  const v = drag ?? (isControlled ? value : internal);
  const pct = ((v - min) / (max - min)) * 100;

  function set(e: ChangeEvent<HTMLInputElement>) {
    const n = Number(e.target.value);
    if (!isControlled) setInternal(n);
    if (onCommit) setDrag(n);
    onChange?.(n);
  }

  function commit() {
    if (!onCommit || drag === null) return;
    setDrag(null);
    onCommit(drag);
  }

  const style: CSSProperties & { "--pct": string } = { "--pct": pct + "%" };

  return (
    <div className="slider">
      <input
        type="range" className="slider__input"
        aria-label={ariaLabel ?? label} disabled={disabled}
        min={min} max={max} step={step} value={v} onChange={set}
        onPointerUp={commit} onKeyUp={commit} onBlur={commit}
        style={style}
      />
      <span className="slider__value">{format(v)}</span>
    </div>
  );
}
