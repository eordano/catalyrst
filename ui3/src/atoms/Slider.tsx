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
  format = (v) => Math.round(v),
  label,
  ariaLabel,
  disabled = false,
}: SliderProps) {
  const [internal, setInternal] = useState(defaultValue);
  const isControlled = value !== undefined;
  const v = isControlled ? value : internal;
  const pct = ((v - min) / (max - min)) * 100;

  function set(e: ChangeEvent<HTMLInputElement>) {
    const n = Number(e.target.value);
    if (!isControlled) setInternal(n);
    onChange?.(n);
  }

  const style: CSSProperties & { "--pct": string } = { "--pct": pct + "%" };

  return (
    <div className="slider">
      <input
        type="range" className="slider__input"
        aria-label={ariaLabel ?? label} disabled={disabled}
        min={min} max={max} step={step} value={v} onChange={set}
        style={style}
      />
      <span className="slider__value">{format(v)}</span>
    </div>
  );
}
