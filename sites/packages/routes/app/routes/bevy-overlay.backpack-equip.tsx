import { useEffect, useRef } from "react";
import { data, useFetcher } from "react-router";

import { toErrorMessage } from "@core/lib/errors";
import { loadBackpack, type BackpackData } from "@data/lib/catalyst/overlay/backpack.server";
import {
  isEthAddress,
  normalizeAddress,
  type Equipped,
} from "@data/lib/catalyst/overlay/backpack";
import { readVerifiedWallet, type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import ClientStage from "@ui/overlay/panels/ClientStage";
import BackpackEquip from "@features/stories/overlay/backpack-equip/BackpackEquip";
import { type SaveResult } from "@features/stories/overlay/backpack-equip/machine";
import { sendBridge, getDeployIdentity } from "@features/components/bevy-overlay/bridge";
import {
  deployProfileWithSigner,
  buildProfileDeployment,
  computeEntityId,
  type AvatarColor,
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

const FIXTURE_EQUIPPED: Equipped = {
  bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale",
  skinColor: "#e8b48c",
  hairColor: "#5c3824",
  eyeColor: "#3a6ea5",
  wearables: [],
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

  const payload = {
    sid,
    backpack,
    assignment,
    // True exactly when the component below will substitute FIXTURE_EQUIPPED.
    // A fixture-seeded editor must never deploy, even if a wallet signs in
    // mid-session: saving it would write BaseMale over the real profile.
    fixtureSeeded: backpack.equipped === null,
  };

  return wrap(payload);
}

function hexToAvatarColor(raw: FormDataEntryValue | null): AvatarColor | null {
  const m = /^#([0-9a-f]{6})$/i.exec(String(raw ?? "").trim());
  if (!m) return null;
  const n = Number.parseInt(m[1], 16);
  return {
    color: {
      r: ((n >> 16) & 255) / 255,
      g: ((n >> 8) & 255) / 255,
      b: (n & 255) / 255,
    },
  };
}

export async function action({ request }: Route.ActionArgs) {
  const form = await request.formData();
  const address = readVerifiedWallet(request);
  const wearables = form
    .getAll("wearables")
    .map((w) => String(w))
    .filter(Boolean);

  if (!address) {
    return data({ deployed: false, reason: "no-address" } as const, { status: 400 });
  }

  const bodyShape = String(form.get("bodyShape") ?? "").trim();
  const skin = hexToAvatarColor(form.get("skinColor"));
  const hair = hexToAvatarColor(form.get("hairColor"));
  const eyes = hexToAvatarColor(form.get("eyeColor"));

  if (!bodyShape || !skin || !hair || !eyes) {
    return data(
      { deployed: false, reason: "incomplete-avatar" } as const,
      { status: 400 },
    );
  }

  const input: ProfileEntityInput = {
    address,
    avatar: { bodyShape, eyes, hair, skin, wearables },
  };

  try {
    const result = await deployProfileWithSigner(input, { signal: request.signal });
    if (result.deployed) {
      return data({ deployed: true as const, entityId: result.entityId });
    }

    const deployment = buildProfileDeployment(input);
    const entityId = computeEntityId(
      new TextEncoder().encode(JSON.stringify(deployment)),
    );
    return data({ deployed: false as const, reason: result.reason, entityId });
  } catch (err) {
    return data(
      { error: toErrorMessage(err, "profile deploy failed") },
      { status: 502 },
    );
  }
}

export default function BackpackEquipRoute({ loaderData }: Route.ComponentProps) {
  const { sid, backpack, assignment, fixtureSeeded } = loaderData;
  const fetcher = useFetcher<typeof action>();
  const pendingSave = useRef<{
    resolve: (r: SaveResult) => void;
    reject: (e: Error) => void;
  } | null>(null);

  useEffect(() => {
    if (fetcher.state !== "idle" || !pendingSave.current) return;
    const { resolve, reject } = pendingSave.current;
    pendingSave.current = null;
    const out = fetcher.data;
    if (out && "entityId" in out && out.entityId) {
      resolve({ entityId: out.entityId, deployed: out.deployed });
    } else if (out && "error" in out) {
      reject(new Error(out.error));
    } else if (out && "reason" in out) {
      reject(new Error(`profile deploy rejected: ${out.reason}`));
    } else {
      reject(new Error("profile deploy failed"));
    }
  }, [fetcher.state, fetcher.data]);

  // Without a profile read there is no avatar to edit, only a default one. The
  // editor saves what it shows, so offering it here would write a body shape and
  // colours nobody chose over the avatar we failed to load.
  if (backpack.equipped === null && backpack.inventory.status !== "not-connected") {
    return (
      <ClientStage nojs="Enable JavaScript to edit and save your avatar.">
        <div role="alert" className="bp__unavailable">
          <h2>We couldn't load your avatar</h2>
          <p>
            Your profile didn't answer, so we can't show what you're wearing. Nothing was
            read &#x2014; this is not an empty avatar, and editing here would overwrite it.
          </p>
        </div>
      </ClientStage>
    );
  }

  const equipped = backpack.equipped ?? FIXTURE_EQUIPPED;

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
        equipped={equipped}
        inventory={backpack.inventory}
        catalogState={backpack.catalogState}
        canRetry={!fixtureSeeded}
        save={({ wearables, colors }) => {
          if (fixtureSeeded) {
            // Deliberately never reads getDeployIdentity here: signing in
            // mid-session must not make this sample avatar deployable.
            return Promise.reject(
              new Error(
                "this backpack opened without a signed-in wallet, so it shows a sample avatar; sign in and reopen the backpack to save yours",
              ),
            );
          }

          sendBridge("SetAvatar", {
            equip: { wearableUrns: wearables, emoteUrns: [], forceRender: [] },
          });

          const identity = getDeployIdentity();
          if (!identity || !identity.signerAddress) {
            return Promise.reject(
              new Error("no wallet identity; sign in to save your avatar"),
            );
          }
          const body = new FormData();
          body.set("address", identity.signerAddress);
          body.set("bodyShape", equipped.bodyShape);
          body.set("skinColor", colors.skin);
          body.set("hairColor", colors.hair);
          body.set("eyeColor", colors.eye);
          for (const w of wearables) body.append("wearables", w);
          return new Promise<SaveResult>((resolve, reject) => {
            pendingSave.current = { resolve, reject };
            void fetcher.submit(body, { method: "POST" });
          });
        }}
      />
    </ClientStage>
  );
}
