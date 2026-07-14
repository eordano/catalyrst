import { data } from "react-router";

import { loadBackpack, type BackpackData } from "@data/lib/catalyst/overlay/backpack.server";
import { isEthAddress, normalizeAddress } from "@data/lib/catalyst/overlay/backpack";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { trackExposure } from "@core/lib/telemetry/track";
import ClientStage from "@ui/overlay/panels/ClientStage";
import BackpackEquip from "@features/stories/overlay/backpack-equip/BackpackEquip";
import { sendBridge, getDeployIdentity } from "@features/components/bevy-overlay/bridge";
import {
  deployProfileWithSigner,
  buildProfileDeployment,
  computeEntityId,
  type ProfileEntityInput,
} from "@data/lib/catalyst/creator-hub/deploy-profile.server";

import type { Route } from "./+types/bevy-overlay.backpack-equip";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/backpack-equip";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "cl_backpack_equip",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const rawAddr = url.searchParams.get("address");
  const address =
    rawAddr && isEthAddress(rawAddr) ? normalizeAddress(rawAddr) : null;

  let backpack: BackpackData;
  try {
    backpack = await loadBackpack(address, request.signal);
  } catch (err) {
    const reason = (err as Error)?.message ?? "network error";
    backpack = {
      address: address ?? "",
      owned: [],
      catalog: [],
      categories: [],
      // A BaseMale in a default skin tone is a stranger, not this player's
      // avatar. The component below refuses to open the editor on null, which
      // is the only honest answer when the profile never answered.
      equipped: null,
      inventory: { status: "unavailable", reason },
      catalogState: { status: "unavailable", reason },
    };
  }

  trackExposure({
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });

  const payload = { sid, backpack, assignment };

  return wrap(payload);
}

export async function action({ request }: Route.ActionArgs) {
  const form = await request.formData();
  const address =
    String(form.get("address") ?? "").trim() ||
    "0x0000000000000000000000000000000000000000";
  const wearables = form
    .getAll("wearables")
    .map((w) => String(w))
    .filter(Boolean);

  const input: ProfileEntityInput = {
    address,
    avatar: { wearables },
  };

  const result = await deployProfileWithSigner(input, { signal: request.signal });
  if (result.deployed) {
    return data({ deployed: true as const, entityId: result.entityId });
  }

  const deployment = buildProfileDeployment(input);
  const entityId = computeEntityId(
    new TextEncoder().encode(JSON.stringify(deployment)),
  );
  return data({ deployed: false as const, reason: result.reason, entityId });
}

export default function BackpackEquipRoute({ loaderData }: Route.ComponentProps) {
  const { sid, backpack, assignment } = loaderData;

  // Without a profile read there is no avatar to edit, only a default one. The
  // editor saves what it shows, so offering it here would write a body shape and
  // colours nobody chose over the avatar we failed to load.
  if (backpack.equipped === null) {
    return (
      <ClientStage nojs="Enable JavaScript to edit and save your avatar.">
        <div role="alert" className="bp__unavailable">
          <h2>We couldn't load your avatar</h2>
          <p>
            Your profile didn't answer, so we can't show what you're wearing. Nothing was
            read — this is not an empty avatar, and editing here would overwrite it.
          </p>
        </div>
      </ClientStage>
    );
  }

  return (
    <ClientStage nojs="Enable JavaScript to edit and save your avatar.">
      <BackpackEquip
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        catalog={backpack.catalog}
        categories={backpack.categories}
        equipped={backpack.equipped}
        inventory={backpack.inventory}
        catalogState={backpack.catalogState}
        save={async ({ wearables }) => {
          sendBridge("SetAvatar", {
            equip: { wearableUrns: wearables, emoteUrns: [], forceRender: [] },
          });

          const identity = getDeployIdentity();
          const body = new FormData();
          body.set("address", identity?.signerAddress ?? "");
          for (const w of wearables) body.append("wearables", w);
          const res = await fetch(window.location.pathname, { method: "POST", body });
          const out = (await res.json().catch(() => null)) as
            | { deployed: true; entityId: string }
            | { deployed: false; entityId: string }
            | null;
          if (out && out.entityId) {
            return { entityId: out.entityId, deployed: out.deployed };
          }
          return { entityId: "", deployed: false };
        }}
      />
    </ClientStage>
  );
}
