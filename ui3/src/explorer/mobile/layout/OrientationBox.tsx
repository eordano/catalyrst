import type { CSSProperties, ReactNode } from "react";
import "./viewport.css";
import "./layout.css";

export type OrientationBoxProps = {
  children?: ReactNode;
  invert?: boolean;
  gap?: number;
  className?: string;
  style?: CSSProperties;
};

type OrientationBoxVars = CSSProperties & { "--mox-gap": number };

export default function OrientationBox({
  children,
  invert = false,
  gap = 0,
  className,
  style,
}: OrientationBoxProps) {
  const classes = ["mox"];
  if (invert) classes.push("mox--invert");
  if (className) classes.push(className);

  const vars: OrientationBoxVars = { ...style, "--mox-gap": gap };

  return (
    <div className={classes.join(" ")} style={vars}>
      {children}
    </div>
  );
}
