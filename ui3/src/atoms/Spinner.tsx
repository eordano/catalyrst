import type { ComponentPropsWithoutRef, CSSProperties } from "react";
import "./spinner.css";

type SpinnerProps = {
  size?: number;
  color?: string;
} & ComponentPropsWithoutRef<"span">;

export default function Spinner({ size = 28, color, ...rest }: SpinnerProps) {
  const style: CSSProperties & { "--sz": string; "--spinner-arc"?: string } = {
    "--sz": size + "px",
  };
  if (color) style["--spinner-arc"] = color;
  return (
    <span className="spinner" style={style} role="status" aria-label="Loading" {...rest}>
      <svg viewBox="0 0 50 50" width={size} height={size}>
        <circle className="spinner__arc" cx="25" cy="25" r="20" fill="none" strokeWidth="4"
          strokeLinecap="round" strokeDasharray="90 160" />
      </svg>
    </span>
  );
}
