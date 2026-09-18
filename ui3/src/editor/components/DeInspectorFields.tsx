import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import Toggle from "../../atoms/Toggle";
import type { EditorVec } from "../types";
import { nudgeFromKey } from "../transform-nudge";

type NudgeAxisFn = (axis: keyof EditorVec, delta: number) => void;

interface AxisRowProps {
  label: string;
  v: EditorVec;
  axes?: readonly (keyof EditorVec)[];
  readOnly?: boolean;
  onNudge?: NudgeAxisFn;
}

export function AxisRow({ label, v, axes = ["x", "y", "z"], readOnly = false, onNudge }: AxisRowProps) {
  return (
    <div className="eui-prop">
      <span className="plabel">{label}</span>
      <span className="pvalue">
        {axes.map((ax) => (
          <span className="eui-axis" key={ax}>
            <span
              className="ax"
              title={onNudge ? "\u{2191}/\u{2193} nudge \u{B1}1 \u{B7} shift \u{B1}0.01" : "drag to scrub \u{B7} shift for fine"}
            >
              {ax.toUpperCase()}
            </span>
            <input
              className="eui-num"
              aria-label={`${label} ${ax.toUpperCase()}`}
              {...(onNudge ? { value: v[ax] } : { defaultValue: v[ax] })}
              readOnly={readOnly}
              spellCheck={false}
              onKeyDown={
                onNudge
                  ? (e) => {
                      const delta = nudgeFromKey(0, e.key, e.shiftKey);
                      if (delta !== null) {
                        e.preventDefault();
                        onNudge(ax, delta);
                      }
                    }
                  : undefined
              }
            />
          </span>
        ))}
      </span>
    </div>
  );
}

interface PropRowProps {
  label: string;
  htmlFor?: string;
  children?: ReactNode;
}

export function PropRow({ label, htmlFor, children }: PropRowProps) {
  return (
    <div className="eui-prop">
      {htmlFor ? (
        <label className="plabel" htmlFor={htmlFor}>{label}</label>
      ) : (
        <span className="plabel">{label}</span>
      )}
      <span className="pvalue">{children}</span>
    </div>
  );
}

export type NudgeFieldFn = (
  field: "position" | "rotation" | "scale",
  axis: keyof EditorVec,
  delta: number,
) => void;

export type CompValue = Record<string, unknown>;
export type WriteComp = (next: CompValue) => void;

export function atPath(v: unknown, path: string[]): unknown {
  let cur: unknown = v;
  for (const k of path) {
    if (cur === null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[k];
  }
  return cur;
}

export function withPath(v: CompValue, path: string[], leaf: unknown): CompValue {
  if (path.length === 0) return v;
  const head = path[0];
  if (head === undefined) return v;
  const rest = path.slice(1);
  const child = v[head];
  const base = child !== null && typeof child === "object" ? (child as CompValue) : {};
  return { ...v, [head]: rest.length === 0 ? leaf : withPath(base, rest, leaf) };
}

export function rgbToHex(c: unknown): string {
  const o = c !== null && typeof c === "object" ? (c as Record<string, unknown>) : {};
  const ch = (k: string) => {
    const n = o[k];
    const f = typeof n === "number" ? n : 1;
    return Math.max(0, Math.min(255, Math.round(f * 255)))
      .toString(16)
      .padStart(2, "0");
  };
  return `#${ch("r")}${ch("g")}${ch("b")}`;
}

export function hexToRgb(hex: string, a: number): Record<string, number> {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m || m[1] === undefined) return { r: 1, g: 1, b: 1, a };
  const n = parseInt(m[1], 16);
  return {
    r: ((n >> 16) & 255) / 255,
    g: ((n >> 8) & 255) / 255,
    b: (n & 255) / 255,
    a,
  };
}

export function NumField({
  id,
  value,
  onCommit,
}: {
  id: string;
  value: number;
  onCommit?: (n: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const n = Number(draft);
    if (Number.isFinite(n) && n !== value) onCommit?.(n);
    else setDraft(String(value));
  };
  return (
    <input
      id={id}
      className="eui-num"
      value={draft}
      readOnly={onCommit === undefined}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(String(value));
      }}
    />
  );
}

export function TextField({
  id,
  value,
  placeholder,
  onCommit,
  list,
}: {
  id: string;
  value: string;
  placeholder?: string;
  onCommit?: (s: string) => void;
  list?: string;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <input
      id={id}
      className="eui-input"
      value={draft}
      placeholder={placeholder}
      spellCheck={false}
      list={list}
      readOnly={onCommit === undefined}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => draft !== value && onCommit?.(draft)}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        if (e.key === "Escape") setDraft(value);
      }}
    />
  );
}

export function BoolField({
  checked,
  label,
  onCommit,
}: {
  checked: boolean;
  label: string;
  onCommit?: (b: boolean) => void;
}) {
  return (
    <Toggle
      checked={checked}
      ariaLabel={label}
      disabled={onCommit === undefined}
      onChange={(next) => onCommit?.(next)}
    />
  );
}
