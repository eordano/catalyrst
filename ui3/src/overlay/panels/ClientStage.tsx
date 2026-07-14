import type { ReactNode } from "react";

import "./clientstage.css";

type ClientStageProps = {
  nojs: ReactNode;
  children?: ReactNode;
};

export default function ClientStage({ nojs, children }: ClientStageProps) {
  return (
    <div className="client-stage">
      <canvas id="bevy-canvas" className="client-canvas" aria-hidden="true" />

      {children}

      <noscript>
        <p className="client-nojs">{nojs}</p>
      </noscript>
    </div>
  );
}
