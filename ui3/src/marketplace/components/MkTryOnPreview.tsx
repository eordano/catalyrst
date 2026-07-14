import { Suspense, lazy, useState } from "react";

import "./tryonpreview.css";

const WearablePreview = lazy(
  () => import("../../wearable-preview/WearablePreview"),
);

type PreviewStatus = "loading" | "ready" | "empty" | "error";

export default function MkTryOnPreview({ urn }: { urn: string }) {
  const [status, setStatus] = useState<PreviewStatus>("loading");
  const failed = status === "error" || status === "empty";
  return (
    <div className="mktryon" data-status={status}>
      <Suspense
        fallback={
          <div className="mktryon__loading" role="status">
            Loading 3D preview…
          </div>
        }
      >
        <WearablePreview
          urns={[urn]}
          spin
          zoom={1.05}
          background="#161318"
          onStatus={setStatus}
        />
        {status === "loading" ? (
          <div className="mktryon__loading" role="status">
            Loading 3D preview…
          </div>
        ) : null}
        {failed ? (
          <div className="mktryon__error" role="status">
            Preview unavailable for this item
          </div>
        ) : null}
      </Suspense>
      {status === "ready" ? (
        <span className="mktryon__hint">Drag to rotate · shown on a base avatar</span>
      ) : null}
    </div>
  );
}
