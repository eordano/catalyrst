import type { AriaRole, CSSProperties, ReactNode } from "react";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Close } from "../atoms/icons";
import { useDialogKeys } from "./useDialogKeys";
import "./modal.css";

type ModalProps = {
  children?: ReactNode;
  onClose?: () => void;
  width?: number | string;
  className?: string;
  style?: CSSProperties;
  ariaLabel?: string;
  ariaLabelledBy?: string;
  role?: AriaRole;
  closeOnBackdrop?: boolean;
  closeOnEsc?: boolean;
  showClose?: boolean;
  /**
   * `true` (default) -- today's behaviour: the dialog is `createPortal`ed to `document.body`
   * on a `position: fixed; inset: 0` backdrop, it locks body scroll and it traps focus.
   *
   * `false` -- render the same card *in place*, in normal document flow, with no global side
   * effects (no portal, no body scroll lock, no focus trap/restore). Lets several dialogs be
   * laid out side by side on one page -- which is what a Storybook `Catalog` needs, since
   * portalled fixed backdrops stack on top of one another and only the topmost is visible.
   */
  portal?: boolean;
};

export default function Modal({ portal = true, ...props }: ModalProps) {
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);
  if (!portal) return <ModalCard {...props} inline />;
  if (!mounted) return null;
  return createPortal(<ModalCard {...props} />, document.body);
}

function ModalCard({
  children,
  onClose,
  width = 420,
  className = "",
  style,
  ariaLabel,
  ariaLabelledBy,
  role = "dialog",
  closeOnBackdrop = true,
  closeOnEsc = true,
  showClose = true,
  inline = false,
}: Omit<ModalProps, "portal"> & { inline?: boolean }) {
  const cardRef = useRef<HTMLDivElement>(null);
  const autoLabelId = useId();

  useLayoutEffect(() => {
    const card = cardRef.current;
    if (!card || ariaLabel || ariaLabelledBy) return;
    const heading = card.querySelector<HTMLElement>("h1, h2, h3, h4, h5, h6");
    if (!heading) return;
    if (!heading.id) heading.id = autoLabelId;
    card.setAttribute("aria-labelledby", heading.id);
    return () => {
      card.removeAttribute("aria-labelledby");
      if (heading.id === autoLabelId) heading.removeAttribute("id");
    };
  }, [ariaLabel, ariaLabelledBy, autoLabelId, children]);

  useEffect(() => {
    if (inline) return;
    const body = document.body;
    const prevOverflow = body.style.overflow;
    const prevPadding = body.style.paddingRight;
    const gap = window.innerWidth - document.documentElement.clientWidth;
    body.style.overflow = "hidden";
    if (gap > 0) body.style.paddingRight = `${gap}px`;
    return () => {
      body.style.overflow = prevOverflow;
      body.style.paddingRight = prevPadding;
    };
  }, [inline]);

  useDialogKeys(cardRef, closeOnEsc ? onClose : undefined, !inline);

  return (
    <div
      className={"modal__backdrop" + (inline ? " modal__backdrop--inline" : "")}
      onClick={closeOnBackdrop ? onClose : undefined}
    >
      <div
        className={"modal__card" + (className ? " " + className : "")}
        style={{ width, ...style }}
        role={role} aria-modal="true" aria-label={ariaLabel} aria-labelledby={ariaLabelledBy} tabIndex={-1} ref={cardRef}
        onClick={(e) => e.stopPropagation()}
      >
        {onClose && showClose ? (
          <button
            type="button"
            className="modal__close"
            aria-label="Close"
            onClick={onClose}
          >
            <Close />
          </button>
        ) : null}
        {children}
      </div>
    </div>
  );
}
