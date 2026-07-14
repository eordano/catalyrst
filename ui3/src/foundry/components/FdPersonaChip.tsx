import type { ReactNode } from "react";
import { AvatarStage } from "../../explorer/components/AvatarPreview";
import {
  AVATAR_EYE_COLORS,
  AVATAR_HAIR_COLORS,
  AVATAR_SKIN_COLORS,
  BODY_SHAPE_URNS,
  DEFAULT_WEARABLES,
  type BodyId,
} from "../../data/randomIdentity";
import "./fdpersonachip.css";

// The one attribution atom. It renders exactly what the row honestly holds and
// nothing more: a claimed persona (name, optionally its live avatar), an
// anonymous visitor badge, or a source label for an imported row that has no
// actor. There is no stock face and no invented name — a name only appears when
// a persona was claimed, and the avatar is rendered live from the real base
// catalog, degrading to the badge/initial when it cannot load.

export type FdAvatarSpec = {
  body: BodyId;
  skin: number;
  hair: number;
  eyes: number;
};

/** The outfit the live previewer renders — built from the real base catalog, the
 *  same shape FdAvatarCrowd hands its base avatars. */
export function fdOutfit(spec: FdAvatarSpec) {
  return {
    bodyShape: BODY_SHAPE_URNS[spec.body],
    wearables: DEFAULT_WEARABLES[spec.body],
    skin: { color: AVATAR_SKIN_COLORS[spec.skin % AVATAR_SKIN_COLORS.length] },
    hair: { color: AVATAR_HAIR_COLORS[spec.hair % AVATAR_HAIR_COLORS.length] },
    eyes: { color: AVATAR_EYE_COLORS[spec.eyes % AVATAR_EYE_COLORS.length] },
  };
}

/** Reads a persona's stored avatar jsonb into a spec, or null when it holds no
 *  usable selection (an unset avatar is rendered as no avatar, never a default). */
export function fdAvatarSpec(
  avatar: Record<string, unknown> | null | undefined,
): FdAvatarSpec | null {
  if (!avatar) return null;
  const body = avatar.body === "B" ? "B" : avatar.body === "A" ? "A" : null;
  if (body === null) return null;
  const idx = (v: unknown): number =>
    typeof v === "number" && Number.isFinite(v) && v >= 0 ? Math.floor(v) : 0;
  return { body, skin: idx(avatar.skin), hair: idx(avatar.hair), eyes: idx(avatar.eyes) };
}

// The three honest states an actor can be in. A raw sid is never one of these.
export type FdChipActor =
  | { name: string; avatar?: FdAvatarSpec | null }
  | { badge: string }
  | { source: string };

export type FdPersonaChipProps = {
  actor: FdChipActor;
  /** Render the live avatar next to the name (named actors only). */
  showAvatar?: boolean;
  href?: string;
  className?: string;
};

export default function FdPersonaChip({
  actor,
  showAvatar = false,
  href,
  className = "",
}: FdPersonaChipProps) {
  const cls = "fd-chip-persona" + (className ? " " + className : "");

  let inner: ReactNode;
  if ("name" in actor) {
    const spec = showAvatar ? actor.avatar ?? null : null;
    inner = (
      <>
        {spec ? (
          <AvatarStage
            className="fd-chip-persona__avatar"
            label={`${actor.name}'s avatar`}
            outfit={fdOutfit(spec)}
            emote="idle"
            pauseOffscreen
          />
        ) : null}
        <span className="fd-chip-persona__name">{actor.name}</span>
      </>
    );
  } else if ("badge" in actor) {
    inner = (
      <>
        <span className="fd-chip-persona__dot" aria-hidden="true" />
        <span className="fd-chip-persona__badge">
          visitor <span className="fd-chip-persona__hex">{actor.badge}</span>
        </span>
      </>
    );
  } else {
    inner = <span className="fd-chip-persona__source">{actor.source}</span>;
  }

  if (href) {
    return (
      <a className={cls + " fd-chip-persona--link"} href={href}>
        {inner}
      </a>
    );
  }
  return <span className={cls}>{inner}</span>;
}
