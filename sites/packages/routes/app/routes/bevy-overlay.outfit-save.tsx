import { data } from "react-router";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { loadOutfitSaveData } from "@data/lib/catalyst/overlay/outfit-save.server";
import type { OutfitSaveData } from "@data/lib/catalyst/overlay/outfit-save";
import OutfitSaveWizard from "@features/stories/overlay/outfit-save/OutfitSaveWizard";
import { getDeployIdentity } from "@features/components/bevy-overlay/bridge";
import {
  deployProfileWithSigner,
  type ProfileEntityInput,
} from "@data/lib/catalyst/creator-hub/deploy-profile.server";
import type { SaveFn } from "@features/stories/overlay/outfit-save/machine";

import type { Route } from "./+types/bevy-overlay.outfit-save";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/outfit-save";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "cl_outfit_save_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const slotRaw = url.searchParams.get("slot");
  const slot = slotRaw != null ? Number.parseInt(slotRaw, 10) : null;
  const address = url.searchParams.get("address")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const seedAddress = address ?? "0x0000000000000000000000000000000000000000";
  const outfitData = await loadOutfitSaveData(seedAddress).catch(
    () => null as OutfitSaveData | null,
  );

  const payload = { sid, step, slot, assignment, outfitData };
  return wrap(payload);
}

export async function action({ request }: Route.ActionArgs) {
  const form = await request.formData();
  const address = String(form.get("address") ?? "").trim();
  const wearables = form
    .getAll("wearables")
    .map((w) => String(w))
    .filter(Boolean);

  if (!address) {
    return data({ deployed: false, reason: "no-address" } as const, { status: 400 });
  }

  const input: ProfileEntityInput = {
    address,
    avatar: { wearables },
  };

  const result = await deployProfileWithSigner(input, { signal: request.signal });
  return data(result);
}

export default function BevyOverlayOutfitSave({ loaderData }: Route.ComponentProps) {
  const { sid, step, slot, assignment, outfitData } = loaderData;

  const data: OutfitSaveData = outfitData ?? {
    address: "0x0000000000000000000000000000000000000000",
    profileEmpty: true,
    freeSlots: 5,
    totalSlots: 10,
    equipped: {
      bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale",
      wearables: [],
    },
    namesForExtraSlots: [],
    outfits: [],
    source: "fixture",
  };

  const save: SaveFn = async ({ slot: targetSlot, name, outfit }) => {
    const identity = getDeployIdentity();
    if (identity && identity.signerAddress) {
      const body = new FormData();
      body.set("address", identity.signerAddress);
      for (const w of outfit.wearables) body.append("wearables", w);
      await fetch(window.location.pathname, { method: "POST", body }).catch(() => {
      });
      return { slot: targetSlot, name, simulated: true };
    }
    await new Promise((r) => setTimeout(r, 350));
    return { slot: targetSlot, name, simulated: true };
  };

  return (
    <main className="bevy-overlay-outfit-save">
      <OutfitSaveWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        seed={{
          equipped: data.equipped,
          freeSlots: data.freeSlots,
          totalSlots: data.totalSlots,
          namesForExtraSlots: data.namesForExtraSlots,
        }}
        profile={outfitData?.address}
        initialStep={step ?? undefined}
        initialSlot={slot ?? undefined}
        save={save}
      />
    </main>
  );
}
