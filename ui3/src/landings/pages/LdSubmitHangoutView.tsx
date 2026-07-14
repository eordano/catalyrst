import type { ReactNode } from "react";

import "./ldsubmithangoutview.css";

type LdSubmitHangoutViewProps = {
  children?: ReactNode;
};

export default function LdSubmitHangoutView({
  children = undefined,
}: LdSubmitHangoutViewProps) {
  return <main className="submit-hangout-route">{children}</main>;
}
