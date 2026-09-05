import { useCallback } from "react";

import StCastNotFound from "@ui/web/pages/StCastNotFound";

import { track, type TrackContext } from "@core/lib/telemetry/track";

const GO_HOME_CLASS = "stcastnotfound__btn";
const VIEW_DOCS_CLASS = "stcastnotfound__link";

export const HOME_HREF = "https://catalyst.example.com";
export const DOCS_HREF = "https://docs.decentraland.org/creator/worlds/cast/";

export type CastNotFoundProps = {
  trackCtx: TrackContext;
  from: "streamer" | "watcher" | "unknown";
  reason: "missing" | "malformed" | "expired" | "ended";
  onTrack?: typeof track;
};

export default function CastNotFound({
  trackCtx,
  from,
  reason,
  onTrack = track,
}: CastNotFoundProps) {
  const onClickCapture = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const target = e.target as HTMLElement | null;
      const anchor = target?.closest?.("a");
      if (!anchor) return;
      if (anchor.classList.contains(GO_HOME_CLASS)) {
        onTrack("cast_not_found_go_home", { from, reason }, trackCtx);
      } else if (anchor.classList.contains(VIEW_DOCS_CLASS)) {
        onTrack("cast_not_found_view_docs", { from, reason }, trackCtx);
      }
    },
    [onTrack, from, reason, trackCtx],
  );

  const NotFound = StCastNotFound as unknown as React.ComponentType<{
    homeHref?: string;
    docsHref?: string;
  }>;

  return (
    <div className="cast-not-found" onClickCapture={onClickCapture}>
      <NotFound homeHref={HOME_HREF} docsHref={DOCS_HREF} />
    </div>
  );
}
