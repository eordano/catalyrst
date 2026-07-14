import type { ComponentPropsWithoutRef } from "react";
import "./button.css";

type ButtonLook = {
  variant?: "primary" | "secondary" | "ghost";
  size?: "sm" | "md" | "lg";
};

export type ButtonProps =
  | (ButtonLook & { as?: "button" } & ComponentPropsWithoutRef<"button">)
  | (ButtonLook & { as: "a" } & ComponentPropsWithoutRef<"a">);

function classes(variant: string, size: string, className: string, extra = "") {
  return (
    "btn btn--" + variant + " btn--" + size + extra + (className ? " " + className : "")
  );
}

export default function Button(props: ButtonProps) {
  if (props.as === "a") {
    const { as: _as, variant = "primary", size = "md", className = "", children, ...rest } = props;
    // An anchor cannot be `disabled`; the gated case is aria-disabled + a class
    // that paints the disabled skin and swallows pointer events.
    const gated = rest["aria-disabled"] === true || rest["aria-disabled"] === "true";
    return (
      <a className={classes(variant, size, className, gated ? " is-disabled" : "")} {...rest}>
        {children}
      </a>
    );
  }

  const {
    as: _as,
    variant = "primary",
    size = "md",
    disabled = false,
    type = "button",
    className = "",
    children,
    ...rest
  } = props;
  return (
    <button
      type={type}
      className={classes(variant, size, className)}
      disabled={disabled}
      {...rest}
    >
      {children}
    </button>
  );
}
