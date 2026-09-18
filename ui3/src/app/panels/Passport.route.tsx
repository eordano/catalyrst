import type { QueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import WearablePreview from "../../wearable-preview/WearablePreview";
import type { AvatarStatus } from "../../wearable-preview/avatar";
import Passport, { type PassportLink } from "../../explorer/pages/Passport";
import {
  usePassport,
  prefetchPassport,
  resolveSelfAddress,
} from "../../data/hooks/useProfile";
import { baseItemUrn, color3ToHex, hexToColor3 } from "../../data/catalyst/backpack";
import { catalystBase } from "../../data/catalyst/client";
import { normalizeAddress } from "../../data/catalyst/profile";
import { useOwnedWearables } from "../../data/hooks/useOwnedItems";
import { useBridgeState } from "../../overlay/bridge";

const FULL_HEX_ADDR = /^0x[0-9a-fA-F]{40}$/;
const shortAddr = (v: string) => `${v.slice(0, 5)}\u{2026}${v.slice(-4)}`;

export function prefetch(queryClient: QueryClient, address?: string | null) {
  try {
    prefetchPassport(queryClient, address === undefined ? resolveSelfAddress() : address);
  } catch {
  }
}

export default function PassportPanel() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const identity = useBridgeState((s) => s.identity);
  const avatarLoadout = useBridgeState((s) => s.avatarLoadout);
  const avatarBase = useBridgeState((s) => s.avatarBase);
  const [avatarStatus, setAvatarStatus] = useState<AvatarStatus>("loading");
  const [avatarAttempt, setAvatarAttempt] = useState(0);

  const self = resolveSelfAddress();
  const targetParam = params.get("address");
  const target =
    targetParam && FULL_HEX_ADDR.test(targetParam) ? normalizeAddress(targetParam) : null;
  const isSelf = !target || (!!self && target === self);
  const address = isSelf ? self || identity.address || null : target;

  const { profile, badges, photos, isLive, isLoading } = usePassport(address);

  const wearables = useOwnedWearables(address);
  const equipped = useMemo(() => {
    const d = wearables.data;
    const byUrn = new Map((d?.catalog ?? []).map((w) => [baseItemUrn(w.urn), w]));
    const urns = (isSelf ? avatarLoadout?.wearables : undefined) ?? d?.equipped?.wearables ?? [];
    return urns
      .map((urn) => byUrn.get(baseItemUrn(urn)))
      .filter((w): w is NonNullable<typeof w> => w != null);
  }, [wearables.data, avatarLoadout, isSelf]);

  const outfit = useMemo(() => {
    const saved = wearables.data?.equipped;
    const live = isSelf ? avatarBase : null;
    const loadout = isSelf ? avatarLoadout : null;
    const bodyShape = live?.bodyShapeUrn || loadout?.bodyShape || saved?.bodyShape;
    if (!bodyShape) return null;
    return {
      bodyShape,
      wearables: loadout?.wearables ?? saved?.wearables ?? [],
      skin: { color: live?.skinColor ?? hexToColor3(saved?.skinColor ?? "#c98c63") },
      hair: { color: live?.hairColor ?? hexToColor3(saved?.hairColor ?? "#5c3824") },
      eyes: { color: live?.eyesColor ?? hexToColor3(saved?.eyeColor ?? "#3a6ea5") },
    };
  }, [wearables.data, isSelf, avatarBase, avatarLoadout]);

  const rawName = isSelf
    ? identity.name || (isLive && profile.name) || ""
    : profile.name || "";
  const name = FULL_HEX_ADDR.test(rawName) ? shortAddr(rawName) : rawName;
  const tag = profile.hasClaimedName
    ? undefined
    : isSelf
      ? profile.tag || identity.tag || undefined
      : profile.tag || undefined;
  const wallet = isSelf
    ? identity.wallet || (address ? shortAddr(address) : undefined)
    : address
      ? shortAddr(address)
      : undefined;
  const links = profile.links as PassportLink[];

  const avatarFailed = outfit ? avatarStatus === "error" || avatarStatus === "empty" : !wearables.isPending;
  const avatarPreview = <div className="ps__avatar-model" aria-label={`${name || "Explorer"} avatar`}>
    {outfit && <WearablePreview key={`${address}:${avatarAttempt}`} base={catalystBase()} outfit={outfit} controls spin={false} platform={false} zoom={1.05} pitch={8} onStatus={setAvatarStatus} />}
    {avatarFailed ? <div className="ps__avatar-status" role="alert"><p>Avatar could not load.</p><Button variant="secondary" size="sm" onClick={() => { setAvatarStatus("loading"); setAvatarAttempt(value => value + 1); void wearables.refetch(); }}>Retry avatar</Button></div>
      : (!outfit || avatarStatus === "loading") && <div className="ps__avatar-status" role="status" aria-label="Loading avatar"><Spinner size={34} /></div>}
  </div>;

  return (
    <Passport
      loading={isLoading || (!avatarFailed && (!outfit || avatarStatus === "loading"))}
      tab={["badges", "photos"].includes(params.get("section") ?? "") ? params.get("section")! : "overview"}
      onTab={section => setParams(previous => { previous.set("section", section); return previous; }, { replace: true })}
      avatarPreview={avatarPreview}
      identity={{
        name,
        tag,
        address: address ?? undefined,
        wallet,
      }}
      nameColor={profile.nameColor}
      hasClaimedName={profile.hasClaimedName}
      about={profile.bio}
      links={links}
      photos={photos.map((p) => ({
        id: p.id,
        url: p.url,
        thumbnailUrl: p.thumbnailUrl || undefined,
        dateTime: p.dateTime || undefined,
      }))}
      equipped={equipped}
      badges={badges.achieved.map((b) => ({
        ...b,
        tier: b.tier ?? undefined,
        image: b.image ?? undefined,
      }))}
      base={outfit ? {
        bodyShape: outfit.bodyShape,
        skinColor: color3ToHex(outfit.skin.color),
        hairColor: color3ToHex(outfit.hair.color),
        eyeColor: color3ToHex(outfit.eyes.color),
      } : wearables.data?.equipped ?? null}
      isSelf={isSelf}
      onEditAvatar={() => navigate("/backpack")}
      onClose={() => navigate("/")}
    />
  );
}
