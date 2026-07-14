import type { ReactNode } from "react";
import "./fdtable.css";

type FdScrollTableProps = {
  children?: ReactNode;
  className?: string;
  ariaLabel?: string;
};

/** Panel frame + horizontal scroll, so a wide table never scrolls the page. */
export default function FdScrollTable({
  children,
  className = "",
  ariaLabel,
}: FdScrollTableProps) {
  return (
    <div
      className={"fd-scroll" + (className ? " " + className : "")}
      tabIndex={0}
      role="group"
      aria-label={ariaLabel}
    >
      {children}
    </div>
  );
}
