import type { ReactNode } from "react";
import { Caret } from "../atoms/icons";
import "./filterbox.css";

type FilterBoxProps = {
  title?: ReactNode;
  open?: boolean;
  onToggle?: () => void;
  size?: "caps" | "title";
  className?: string;
  children?: ReactNode;
};

export default function FilterBox({
  title,
  open = false,
  onToggle,
  size = "caps",
  className = "",
  children,
}: FilterBoxProps) {
  return (
    <div className={"filterbox filterbox--" + size + (className ? " " + className : "")}>
      <button type="button" className="filterbox__head" aria-expanded={open} onClick={onToggle}>
        <span className="filterbox__title">{title}</span>
        <Caret open={open} className="filterbox__caret" />
      </button>
      {open ? <div className="filterbox__body">{children}</div> : null}
    </div>
  );
}
