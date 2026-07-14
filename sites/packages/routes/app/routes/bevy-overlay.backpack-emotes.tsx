import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { loadBackpackEmotes } from "@data/lib/catalyst/overlay/backpack-emotes.server";
import BackpackEmotesWizard from "@features/stories/overlay/backpack-emotes/BackpackEmotesWizard";

import type { Route } from "./+types/bevy-overlay.backpack-emotes";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/backpack-emotes";

const FALLBACK: Assignment = {
  variant: "slot_first",
  flags: { slotFirst: true },
  experimentKey: "cl_backpack_emotes",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const slotRaw = url.searchParams.get("slot");
  const slot = slotRaw != null && slotRaw !== "" ? Number(slotRaw) : null;
  const urn = url.searchParams.get("urn")?.trim() || null;
  const address = url.searchParams.get("address")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const emotes = await loadBackpackEmotes(address);

  const payload = {
    sid,
    step,
    slot: Number.isNaN(slot) ? null : slot,
    urn,
    address,
    assignment,
    emotes,
  };
  return wrap(payload);
}

export default function BevyOverlayBackpackEmotes({
  loaderData,
}: Route.ComponentProps) {
  const { sid, step, slot, urn, address, assignment, emotes } = loaderData;

  return (
    <main className="bevy-overlay-backpack-emotes">
      <BackpackEmotesWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        profile={address ?? undefined}
        catalog={emotes.catalog}
        loadout={emotes.loadout}
        slotOrder={emotes.slotOrder}
        liveEmpty={emotes.liveEmpty}
        initialStep={step ?? undefined}
        initialSlot={slot}
        initialUrn={urn}
      />
    </main>
  );
}
