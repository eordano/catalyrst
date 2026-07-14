import { useEffect, useRef } from "react";

import { fdAvatarSpec } from "@ui/foundry/components/FdPersonaChip";
import FdPersonPage, { type FdPersonVM } from "@ui/foundry/pages/FdPersonPage";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdperson.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import { personProfile } from "@data/lib/foundry/persona.server";

import type { Route } from "./+types/foundry.people_.$name";

// One claimed persona's page — the creator-identity artifact ("what's your
// GitHub profile page", 08-17 working session). Looked up by the
// case-insensitively-unique display name; a name nobody claimed is a plain
// 404, never a scaffold for a person who does not exist.

export function meta({ loaderData }: Route.MetaArgs) {
  const name = loaderData?.person?.displayName;
  return [{ title: name ? `${name} — The Foundry` : "People — The Foundry" }];
}

export async function loader({ request, params }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const name = decodeURIComponent(params.name ?? "").trim();
  if (!name) throw new Response("Not found", { status: 404 });

  let person: FdPersonVM | null = null;
  let unavailable = false;
  try {
    const profile = await personProfile(name);
    if (profile) {
      person = {
        displayName: profile.displayName,
        avatar: fdAvatarSpec(profile.avatar),
        words: profile.words,
        claimedAt: profile.claimedAt,
        roles: profile.roles,
        lastSeen: profile.lastSeen,
        stewarding: profile.stewarding,
        asks: profile.asks,
        pledges: profile.pledges,
        acts: profile.acts,
      };
    }
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  if (!unavailable && person === null) {
    throw new Response("Not found", { status: 404 });
  }
  return wrap({ badge: sidBadge(sid), person, unavailable });
}

export default function FoundryPerson({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || !d.person) return;
    viewed.current = true;
    track("fd_person_viewed", { name_len: d.person.displayName.length }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  if (!d.person) {
    return <p className="fd-empty">The people records could not be read.</p>;
  }
  return <FdPersonPage person={d.person} />;
}
