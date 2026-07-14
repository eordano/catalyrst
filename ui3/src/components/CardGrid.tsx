import type { ComponentPropsWithoutRef, CSSProperties } from "react";
import "./cardgrid.css";

const len = (v: number | string): string => (typeof v === "number" ? v + "px" : v);

const COL_KEYS = { base: "--cg-cols", lg: "--cg-cols-lg", md: "--cg-cols-md" } as const;

type ColKey = keyof typeof COL_KEYS;
type ResponsiveCols = Partial<Record<ColKey, number>>;

type GridVars = {
  "--cg-gap": string;
  "--cg-min"?: string;
  "--cg-cols"?: string;
  "--cg-cols-lg"?: string;
  "--cg-cols-md"?: string;
  "--cg-minh"?: string;
};

type CardGridProps = {
  min?: number | string;
  gap?: number | string;
  cols?: number | ResponsiveCols;
  minHeight?: number | string;
} & ComponentPropsWithoutRef<"div">;

export default function CardGrid({
  min = "240px",
  gap = "16px",
  cols,
  minHeight,
  className = "",
  style,
  children,
  ...rest
}: CardGridProps) {
  const fixed = cols != null;
  const vars: GridVars = { "--cg-gap": len(gap) };

  if (!fixed) {
    vars["--cg-min"] = len(min);
  } else if (typeof cols === "object") {
    for (const k of Object.keys(COL_KEYS) as ColKey[]) {
      const c = cols[k];
      if (c != null) vars[COL_KEYS[k]] = String(c);
    }
  } else {
    vars["--cg-cols"] = String(cols);
  }
  if (minHeight != null) vars["--cg-minh"] = len(minHeight);

  const cls =
    "cardgrid" +
    (fixed ? " cardgrid--fixed" : "") +
    (minHeight != null ? " cardgrid--reserve" : "") +
    (className ? " " + className : "");

  const mergedStyle: CSSProperties & GridVars = { ...style, ...vars };

  return (
    <div className={cls} style={mergedStyle} {...rest}>
      {children}
    </div>
  );
}
