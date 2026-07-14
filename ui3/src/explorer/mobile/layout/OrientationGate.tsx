import type { ReactNode } from "react";
import type { Orientation } from "./orientation";
import { useOrientation } from "./OrientationProvider";
import "./viewport.css";
import "./layout.css";

export type OrientationGateMode = "unmount" | "hide";

export type OrientationGateProps = {
  children: ReactNode;
  show: Orientation;
  mode?: OrientationGateMode;
  className?: string;
};

export default function OrientationGate({
  children,
  show,
  mode = "unmount",
  className,
}: OrientationGateProps) {
  const orientation = useOrientation();

  if (mode === "unmount") {
    if (orientation !== show) return null;
    return <>{children}</>;
  }

  const classes = ["mog", show === "portrait" ? "mog--portrait" : "mog--landscape"];
  if (className) classes.push(className);

  return <div className={classes.join(" ")}>{children}</div>;
}
