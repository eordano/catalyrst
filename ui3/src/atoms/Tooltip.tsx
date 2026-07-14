import type { ReactNode } from "react";
import "./tooltip.css";

type TooltipSide = "right" | "left" | "top" | "bottom";

type TooltipProps = {
  label: string;
  shortcut?: string;
  side?: TooltipSide;
  className?: string;
  children: ReactNode;
};

export default function Tooltip({ label, shortcut, side = "right", className, children }: TooltipProps) {
  return (
    <span className={"tt__wrap" + (className ? " " + className : "")}>
      {children}
      <span className={"tt__tip tt__tip--" + side} role="tooltip">
        {label}
        {shortcut ? <span className="tt__shortcut">[{shortcut}]</span> : null}
      </span>
    </span>
  );
}
