import { useState } from "react";
import WearablePreview from "../../wearable-preview/WearablePreview";
import type { AvatarSceneOptions } from "../../wearable-preview/avatar";
import "./avatarpreview.css";

export const SKIN = ["#f5d6c0", "#e8b48c", "#c98c63", "#8d5a3c", "#5c3824"];
export const HAIRC = ["#1a1a1a", "#5c3824", "#b06a2c", "#d9a441", "#9b2d2d", "#3a6ea5"];

export type AvatarStageProps = Pick<
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
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  return (
    <div
      className={"avatar-stage" + (className ? " " + className : "")}
      role={failed ? undefined : "img"}
      aria-label={failed ? undefined : label}
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
          onStatus={(s) => setFailed(s === "error")}
        />
      )}
      {failed && (
        <p className="avatar-stage__gate" role="status">
          Avatar preview failed to load.
          <button
            type="button"
            className="avatar-stage__retry"
            onClick={() => {
              setAttempt((n) => n + 1);
              setFailed(false);
            }}
          >
            Retry
          </button>
        </p>
      )}
    </div>
  );
}

export function Swatches({ colors }: { colors: readonly string[] }) {
  return (
    <div className="bp__swatches">
      {colors.map((c) => <span key={c} className="bp__sw" style={{ background: c }} />)}
    </div>
  );
}
