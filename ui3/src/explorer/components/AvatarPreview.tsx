import { useState } from "react";
import WearablePreview from "../../wearable-preview/WearablePreview";
import type { AvatarSceneOptions, AvatarStatus } from "../../wearable-preview/avatar";
import "./avatarpreview.css";

export const SKIN = ["#f5d6c0", "#e8b48c", "#c98c63", "#8d5a3c", "#5c3824"];
export const HAIRC = ["#1a1a1a", "#5c3824", "#b06a2c", "#d9a441", "#9b2d2d", "#3a6ea5"];

type AvatarStageProps = Pick<
  AvatarSceneOptions,
  "profile" | "urns" | "body" | "outfit" | "emote" | "emotes"
> & { className?: string; label?: string; pauseOffscreen?: boolean };

export function AvatarStage({
  className = "",
  label = "Avatar preview",
  profile,
  urns,
  body,
  outfit,
  emote,
  emotes,
  pauseOffscreen,
}: AvatarStageProps) {
  const [status, setStatus] = useState<AvatarStatus>("loading");
  const failed = status === "error" || status === "empty";
  const [attempt, setAttempt] = useState(0);
  return (
    <div
      className={"avatar-stage" + (className ? " " + className : "")}
      role={status === "ready" ? "img" : undefined}
      aria-label={status === "ready" ? label : undefined}
      aria-busy={status === "loading"}
    >
      {!failed && (
        <WearablePreview
          key={attempt}
          controls={false}
          platform
          profile={profile}
          urns={urns}
          body={body}
          outfit={outfit}
          emote={emote}
          emotes={emotes}
          pauseOffscreen={pauseOffscreen}
          onStatus={setStatus}
        />
      )}
      {status === "loading" && <p className="avatar-stage__gate" role="status">Loading avatar&#x2026;</p>}
      {failed && (
        <p className="avatar-stage__gate" role="status">
          Avatar preview failed to load.
          <button
            type="button"
            className="avatar-stage__retry"
            onClick={() => {
              setAttempt((n) => n + 1);
              setStatus("loading");
            }}
          >
            Retry
          </button>
        </p>
      )}
    </div>
  );
}
