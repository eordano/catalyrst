import type { ComponentPropsWithoutRef } from "react";
import "./button.css";

type ButtonProps = {
  variant?: "primary" | "secondary" | "ghost";
  size?: "sm" | "md" | "lg";
} & ComponentPropsWithoutRef<"button">;

export default function Button({
  variant = "primary",
  size = "md",
  disabled = false,
  onClick,
  type = "button",
  className = "",
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={
        "btn btn--" + variant + " btn--" + size + (className ? " " + className : "")
      }
      disabled={disabled}
      onClick={onClick}
      {...rest}
    >
      {children}
    </button>
  );
}
