import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./tooltip.css";

type TooltipSide = "right" | "left" | "top" | "bottom";

type TooltipProps = {
  label: string;
  shortcut?: string;
  side?: TooltipSide;
  className?: string;
  children: ReactNode;
  portal?: boolean;
};

export default function Tooltip({ label, shortcut, side = "right", className, children, portal = false }: TooltipProps) {
  const ref = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0 });
  useEffect(() => {
    if (!portal || !open) return;
    const close = () => setOpen(false);
    const outside = (event: Event) => {
      if (event.target instanceof Node && !ref.current?.contains(event.target)) close();
    };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("focusin", outside, true);
    document.addEventListener("pointerlockchange", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("focusin", outside, true);
      document.removeEventListener("pointerlockchange", close);
      window.removeEventListener("blur", close);
    };
  }, [portal, open]);
  useEffect(() => {
    if (!portal || !open) return;
    const measure = () => {
      const rect = ref.current?.getBoundingClientRect();
      if (!rect) return;
      setPosition(side === "right" ? { left: rect.right + 12, top: rect.top + rect.height / 2 }
        : side === "left" ? { left: rect.left - 12, top: rect.top + rect.height / 2 }
        : { left: rect.left + rect.width / 2, top: side === "top" ? rect.top - 12 : rect.bottom + 12 });
    };
    measure();
    window.addEventListener("scroll", measure, true);
    window.addEventListener("resize", measure);
    return () => { window.removeEventListener("scroll", measure, true); window.removeEventListener("resize", measure); };
  }, [portal, open, side]);
  const transform = side === "right" ? "translateY(-50%)" : side === "left" ? "translate(-100%, -50%)" : side === "top" ? "translate(-50%, -100%)" : "translateX(-50%)";
  const tip = <span className={"tt__tip tt__tip--" + side} role="tooltip" style={portal ? { position: "fixed", ...position, right: "auto", bottom: "auto", transform, opacity: 1 } : undefined}>
    {label}{shortcut ? <span className="tt__shortcut">[{shortcut}]</span> : null}
  </span>;
  return (
    <span ref={ref} className={"tt__wrap" + (className ? " " + className : "")} onMouseEnter={portal ? () => { if (!document.pointerLockElement) setOpen(true); } : undefined} onMouseLeave={portal ? () => setOpen(false) : undefined} onFocus={portal ? (event) => { if (event.target.matches(":focus-visible") && !document.pointerLockElement) setOpen(true); } : undefined} onBlur={portal ? () => setOpen(false) : undefined}>
      {children}
      {portal ? open && createPortal(tip, document.body) : tip}
    </span>
  );
}
