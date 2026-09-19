import type { QueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";

import Backpack from "../../explorer/pages/Backpack";
import { useBridgeState, FALLBACK_STATE } from "../../overlay/bridge";
import {
  useOwnedItems,
  useOutfits,
  prefetchOwnedItems,
} from "../../data/hooks/useOwnedItems";
import {
  saveOutfits,
  hexToColor3,
  color3ToHex,
  fetchEmoteGlbUrl,
} from "../../data/catalyst/backpack";
import WearablePreview from "../../wearable-preview/WearablePreview";
import Spinner from "../../atoms/Spinner";
import { catalystBase } from "../../data/catalyst/client";
import type { AvatarStatus } from "../../wearable-preview/avatar";

type PreviewBase = {
  bodyShape?: string;
  skinColor?: string;
  hairColor?: string;
  eyeColor?: string;
};

const DEFAULT_BODY = "urn:decentraland:off-chain:base-avatars:BaseMale";
const DEFAULT_SKIN = "#c98c63";
const DEFAULT_HAIR = "#5c3824";
const DEFAULT_EYES = "#3a6ea5";

function guessAddress(): string | null {
  if (typeof window !== "undefined") {
    const id = window.dclDeployIdentity;
    if (id && !id.isGuest && id.signerAddress) return id.signerAddress;
  }
  return FALLBACK_STATE.identity.address;
}

export function prefetch(queryClient: QueryClient) {
  try {
    prefetchOwnedItems(queryClient, guessAddress());
  } catch {
  }
}

export default function BackpackPanel() {
  const identity = useBridgeState((s) => s.identity);
  const avatarBase = useBridgeState(s => s.avatarBase);
  const avatarLoadout = useBridgeState((s) => s.avatarLoadout);
  const address = identity?.address ?? guessAddress();

  const { wearables, emotes, isLoading, isError, error } =
    useOwnedItems(address);
  const outfitsQuery = useOutfits(address);

  const [previewUrns, setPreviewUrns] = useState<string[] | null>(null);
  const [previewBase, setPreviewBase] = useState<PreviewBase | null>(null);
  const [emote, setEmote] = useState({ value: "idle", nonce: 0 });
  const [previewStatus, setPreviewStatus] = useState<AvatarStatus>("loading");
  const [previewAttempt, setPreviewAttempt] = useState(0);

  useEffect(() => {
    setPreviewUrns(null);
    setPreviewBase(null);
  }, [address]);

  const dataProps = useMemo(() => {
    const w = wearables.data;
    const e = emotes.data;
    const catEquipped = avatarBase ? {
      ...w?.equipped,
      ...(avatarBase.bodyShapeUrn ? { bodyShape: avatarBase.bodyShapeUrn } : {}),
      ...(avatarBase.skinColor ? { skinColor: color3ToHex(avatarBase.skinColor) } : {}),
      ...(avatarBase.hairColor ? { hairColor: color3ToHex(avatarBase.hairColor) } : {}),
      ...(avatarBase.eyesColor ? { eyeColor: color3ToHex(avatarBase.eyesColor) } : {}),
    } : w?.equipped ?? null;
    const equipped = avatarLoadout
      ? {
          ...(catEquipped ?? {}),
          wearables: avatarLoadout.wearables,
          bodyShape: avatarLoadout.bodyShape ?? catEquipped?.bodyShape,
          emotes: avatarLoadout.emotes ?? catEquipped?.emotes,
        }
      : catEquipped;
    return {
      address,
      owned: w?.owned ?? [],
      ownedUrns: w?.ownedUrns ?? [],
      catalog: (w?.catalog ?? []).map((it) => ({
        ...it,
        creator: it.creator ?? undefined,
      })),
      categories: w?.categories ?? [],
      equipped,
      ownedEmpty: w?.ownedEmpty ?? true,
      emotes: e?.owned ?? [],
      emoteCatalog: e?.catalog ?? [],
      emoteLoadout: e?.loadout ?? [],
      emoteSlotOrder: e?.slotOrder ?? [],
      source: w?.source ?? e?.source ?? "live",
      loading: isLoading,
      error: isError ? error?.message ?? "Failed to load backpack" : null,
      outfits: outfitsQuery.data ?? [],
    };
  }, [
    wearables.data,
    emotes.data,
    outfitsQuery.data,
    address,
    avatarLoadout,
    avatarBase,
    isLoading,
    isError,
    error,
  ]);

  const equipped = dataProps.equipped;
  const outfitKnown = !isLoading || avatarLoadout != null;
  const outfit = useMemo(
    () => ({
      bodyShape: previewBase?.bodyShape ?? equipped?.bodyShape ?? DEFAULT_BODY,
      wearables: previewUrns ?? equipped?.wearables ?? [],
      skin: {
        color: hexToColor3(previewBase?.skinColor ?? equipped?.skinColor ?? DEFAULT_SKIN),
      },
      hair: {
        color: hexToColor3(previewBase?.hairColor ?? equipped?.hairColor ?? DEFAULT_HAIR),
      },
      eyes: {
        color: hexToColor3(previewBase?.eyeColor ?? equipped?.eyeColor ?? DEFAULT_EYES),
      },
    }),
    [previewBase, previewUrns, equipped],
  );

  const playEmoteOnPreview = async (urn: string) => {
    let url = null;
    try {
      url = await fetchEmoteGlbUrl(urn);
    } catch {
    }
    const value = url || String(urn).split(":").pop() || "";
    setEmote((e) => ({ value, nonce: e.nonce + 1 }));
  };

  return (
    <Backpack
      avatarPreview={
        <div className="bp__avatar-preview">
          {outfitKnown ? (
            <WearablePreview
              key={previewAttempt}
              base={catalystBase()}
              outfit={outfit}
              platform
              spin={false}
              controls
              zoom={1.05}
              pitch={8}
              emote={emote.value}
              emoteNonce={emote.nonce}
              onStatus={setPreviewStatus}
            />
          ) : null}
          {previewStatus === "loading" || !outfitKnown ? (
            <div className="bp__avatar-loading" role="status" aria-label={"Loading avatar\u{2026}"}>
              <Spinner size={34} color="rgba(255,255,255,0.72)" aria-hidden />
            </div>
          ) : null}
          {outfitKnown && (previewStatus === "error" || previewStatus === "empty") && (
            <div className="bp__avatar-loading" role="alert">
              <p>Avatar preview could not load.</p>
              <button type="button" onClick={() => { setPreviewStatus("loading"); setPreviewAttempt((n) => n + 1); }}>Retry preview</button>
            </div>
          )}
        </div>
      }
      avatarName={identity?.name ?? ""}
      onOutfitsChange={(next) => saveOutfits(address, next)}
      onEquippedChange={setPreviewUrns}
      onBaseChange={(b) =>
        setPreviewBase({
          bodyShape: b.bodyShape,
          skinColor: b.skinColor,
          hairColor: b.hairColor,
          eyeColor: b.eyeColor,
        })
      }
      onPlayEmote={playEmoteOnPreview}
      {...dataProps}
    />
  );
}
