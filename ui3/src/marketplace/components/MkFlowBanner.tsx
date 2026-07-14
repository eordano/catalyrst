import type { ReactNode } from "react";

import "./flowbanner.css";

type MkFlowBannerProps = {
  children?: ReactNode;
};

export default function MkFlowBanner({ children = undefined }: MkFlowBannerProps) {
  return (
    <p className="mkflow-banner" role="note">
      {children}
    </p>
  );
}
