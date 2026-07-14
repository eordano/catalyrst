import { useEffect, useState } from "react";
import { AvatarStage } from "../../explorer/components/AvatarPreview";
import {
  AVATAR_EYE_COLORS,
  AVATAR_HAIR_COLORS,
  AVATAR_SKIN_COLORS,
  BODY_SHAPE_URNS,
  DEFAULT_WEARABLES,
  type BodyId,
} from "../../data/randomIdentity";
import "./fdavatarcrowd.css";

// Decentraland's own base avatars — the body shapes and default wearables every
// new account is handed — loaded from the content server by the same previewer
// the backpack and passport use. They are nobody in particular, and the page
// never dresses them up as a count of who is online.
type CrewMember = {
  id: string;
  body: BodyId;
  skin: number;
  hair: number;
  eyes: number;
  emote: string;
};

// Each avatar plays a looping emote clip — a crowd stands around waving and
// dancing, never in a T-pose.
const CREW: readonly CrewMember[] = [
  { id: "c1", body: "A", skin: 0, hair: 2, eyes: 1, emote: "wave" },
  { id: "c2", body: "B", skin: 2, hair: 0, eyes: 0, emote: "dance" },
  { id: "c3", body: "A", skin: 4, hair: 5, eyes: 3, emote: "idle" },
  { id: "c4", body: "B", skin: 1, hair: 3, eyes: 2, emote: "wave" },
  { id: "c5", body: "A", skin: 3, hair: 1, eyes: 0, emote: "dance" },
];

function outfitFor(m: CrewMember) {
  return {
    bodyShape: BODY_SHAPE_URNS[m.body],
    wearables: DEFAULT_WEARABLES[m.body],
    skin: { color: AVATAR_SKIN_COLORS[m.skin % AVATAR_SKIN_COLORS.length] },
    hair: { color: AVATAR_HAIR_COLORS[m.hair % AVATAR_HAIR_COLORS.length] },
    eyes: { color: AVATAR_EYE_COLORS[m.eyes % AVATAR_EYE_COLORS.length] },
  };
}

// A narrow viewport cannot fit five tiles, so mount four rather than mounting
// five and hiding one with CSS — a hidden tile still boots its own renderer and
// fetches its outfit. Starts at the wide count so the server render and the first
// client render agree; the effect narrows it after mount, before the fifth tile's
// deferred renderer has a chance to fetch anything.
function useCrowdSize(): number {
  const [narrow, setNarrow] = useState(false);
  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia("(max-width: 640px)");
    const sync = () => setNarrow(mq.matches);
    sync();
    mq.addEventListener("change", sync);
    return () => mq.removeEventListener("change", sync);
  }, []);
  return narrow ? 4 : 5;
}

export type FdAvatarCrowdViewer = { address: string; name: string };

export type FdAvatarCrowdProps = {
  viewer?: FdAvatarCrowdViewer | null;
  caption: string;
};

export default function FdAvatarCrowd({
  viewer = null,
  caption,
}: FdAvatarCrowdProps) {
  const total = useCrowdSize();
  const crew = viewer ? CREW.slice(1, total) : CREW.slice(0, total);
  return (
    <div className="fd-crowd">
      <ul className="fd-crowd__row" aria-label="Decentraland avatars">
        {viewer ? (
          <li className="fd-crowd__tile fd-crowd__tile--you">
            <AvatarStage
              className="fd-crowd__stage"
              label="Your Decentraland avatar"
              profile={viewer.address}
              emote="wave"
              pauseOffscreen
            />
            <p className="fd-crowd__tag">{viewer.name || "You"}</p>
          </li>
        ) : null}
        {crew.map((m, i) => (
          <li className="fd-crowd__tile" key={m.id}>
            <AvatarStage
              className="fd-crowd__stage"
              label={`Decentraland base avatar ${i + 1}`}
              outfit={outfitFor(m)}
              emote={m.emote}
              pauseOffscreen
            />
          </li>
        ))}
      </ul>
      <p className="fd-crowd__caption">{caption}</p>
    </div>
  );
}
