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
  fetchEmoteGlbUrl,
} from "../../data/catalyst/backpack";
import WearablePreview from "../../wearable-preview/WearablePreview";
import Spinner from "../../atoms/Spinner";

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
  const avatarLoadout = useBridgeState((s) => s.avatarLoadout);
  const address = identity?.address ?? guessAddress();

  const { wearables, emotes, isLoading, isError, error } =
    useOwnedItems(address);
  const outfitsQuery = useOutfits(address);

  const [previewUrns, setPreviewUrns] = useState<string[] | null>(null);
  const [previewBase, setPreviewBase] = useState<PreviewBase | null>(null);
  const [emote, setEmote] = useState({ value: "idle", nonce: 0 });
  const [previewLoading, setPreviewLoading] = useState(true);

  useEffect(() => {
    setPreviewUrns(null);
    setPreviewBase(null);
  }, [address]);

  const dataProps = useMemo(() => {
    const w = wearables.data;
    const e = emotes.data;
    const catEquipped = w?.equipped ?? null;
    const equipped = avatarLoadout?.wearables?.length
      ? {
          ...(catEquipped ?? {}),
          wearables: avatarLoadout.wearables,
          bodyShape: avatarLoadout.bodyShape ?? catEquipped?.bodyShape,
          emotes:
            (avatarLoadout.emotes?.length ?? 0) > 0
              ? avatarLoadout.emotes
              : catEquipped?.emotes,
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
    isLoading,
    isError,
    error,
  ]);

  const equipped = dataProps.equipped;
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
          <WearablePreview
            outfit={outfit}
            platform
            spin={false}
            controls
            zoom={1.05}
            pitch={8}
            emote={emote.value}
            emoteNonce={emote.nonce}
            onStatus={(s) => setPreviewLoading(s === "loading")}
          />
          {previewLoading ? (
            <div className="bp__avatar-loading" role="status" aria-label={"Loading avatar\u{2026}"}>
              <Spinner size={34} color="rgba(255,255,255,0.72)" aria-hidden />
            </div>
          ) : null}
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
