import type { ComponentPropsWithoutRef } from "react";
import "./button.css";

type ButtonLook = {
  variant?: "primary" | "secondary" | "ghost";
  size?: "sm" | "md" | "lg";
  tone?: "danger" | "success" | "warning";
};

type ButtonProps =
  | (ButtonLook & { as?: "button" } & ComponentPropsWithoutRef<"button">)
  | (ButtonLook & { as: "a" } & ComponentPropsWithoutRef<"a">);

function classes(variant: string, size: string, className: string, extra = "") {
  return (
    "btn btn--" + variant + " btn--" + size + extra + (className ? " " + className : "")
  );
}

export default function Button(props: ButtonProps) {
  if (props.as === "a") {
    const { as: _as, variant = "primary", size = "md", tone, className = "", children, ...rest } = props;
    const gated = rest["aria-disabled"] === true || rest["aria-disabled"] === "true";
    return (
      <a className={classes(variant, size, className, gated ? " is-disabled" : "")} data-tone={tone} {...rest}>
        {children}
      </a>
    );
  }

  const {
    as: _as,
    variant = "primary",
    size = "md",
    tone,
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
      data-tone={tone}
      disabled={disabled}
      {...rest}
    >
      {children}
    </button>
  );
}
