import { useEffect, useRef } from "react";

import GvTransparency from "@ui/governance/pages/GvTransparency";

import { loadTransparencyData } from "@data/lib/catalyst/governance/transparency";
import {
  fetchAuthorProfiles,
  type AuthorProfile,
} from "@data/lib/catalyst/governance/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoaderWith } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/governance.transparency";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const handle = { agentMarkdown: "transparency" } satisfies AgentMarkdownHandle;

const STORY: StoryId = "governance/transparency";

type ResolvedMember = {
  name: string;
  address: string;
  addressShort: string;
  face: string | null;
  hue: number;
};
type ResolvedCommittee = {
  name: string;
  description: string;
  members: ResolvedMember[];
};

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "with-transparency",
  flags: { showTransparency: true },
  experimentKey: "gv_transparency_page",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap, data: transparency } = await storyLoaderWith(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
    () => loadTransparencyData({ signal: request.signal }),
  );

  const memberAddresses = [
    ...new Set(
      transparency.committees.flatMap((c) => c.members.map((m) => m.address)),
    ),
  ];
  let profiles: Record<string, AuthorProfile> | null = null;
  try {
    profiles = await fetchAuthorProfiles(memberAddresses, {
      signal: request.signal,
    });
  } catch {
    profiles = null;
  }

  const committees: ResolvedCommittee[] = transparency.committees.map((c) => ({
    name: c.name,
    description: c.description,
    members: c.members.map((m) => {
      const resolved = profiles?.[m.address]?.name?.trim();
      return {
        name: resolved || m.addressShort,
        address: m.address,
        addressShort: m.addressShort,
        face: profiles?.[m.address]?.face ?? null,
        hue: m.hue,
      };
    }),
  }));

  const payload = { sid, assignment, transparency, committees };

  return wrap(payload);
}

export default function GovernanceTransparency({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const t = d.transparency;
  const committees =
    d.committees ??
    t.committees.map((c) => ({
      name: c.name,
      description: c.description,
      members: c.members.map((m) => ({
        name: m.name || m.addressShort,
        address: m.address,
        addressShort: m.addressShort,
        face: null,
        hue: m.hue,
      })),
    }));

  const ctx = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "gv_transparency_viewed",
      {
        source: t.source,
        committees: t.committees.length,
      },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.sid]);

  return (
    <div className="governance-transparency-route">
      <GvTransparency
        committees={
          committees as React.ComponentProps<
            typeof GvTransparency
          >["committees"]
        }
      />
    </div>
  );
}

