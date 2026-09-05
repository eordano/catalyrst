import type { CSSProperties, ReactNode } from "react";
import "./viewport.css";
import "./layout.css";

export type SafeAreaEdges = {
  top?: boolean;
  right?: boolean;
  bottom?: boolean;
  left?: boolean;
};

export type SafeAreaMinMargins = {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
};

export type SafeAreaProps = {
  children?: ReactNode;
  defaultMargin?: number;
  edges?: SafeAreaEdges;
  minMargin?: SafeAreaMinMargins;
  className?: string;
  style?: CSSProperties;
};

type SafeAreaVars = CSSProperties & {
  "--msa-default": number;
  "--msa-min-top": number;
  "--msa-min-right": number;
  "--msa-min-bottom": number;
  "--msa-min-left": number;
};

export default function SafeArea({
  children,
  defaultMargin = 0,
  edges,
  minMargin,
  className,
  style,
}: SafeAreaProps) {
  const classes = ["msa"];
  if (edges?.top === false) classes.push("msa--no-top");
  if (edges?.right === false) classes.push("msa--no-right");
  if (edges?.bottom === false) classes.push("msa--no-bottom");
  if (edges?.left === false) classes.push("msa--no-left");
  if (className) classes.push(className);

  const vars: SafeAreaVars = {
    ...style,
    "--msa-default": defaultMargin,
    "--msa-min-top": minMargin?.top ?? 0,
    "--msa-min-right": minMargin?.right ?? 0,
    "--msa-min-bottom": minMargin?.bottom ?? 0,
    "--msa-min-left": minMargin?.left ?? 0,
  };

  return (
    <div className={classes.join(" ")} style={vars}>
      {children}
    </div>
  );
}
