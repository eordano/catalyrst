import { useState } from "react";
import { useNavigate } from "react-router";

import MkManageAssetView from "@ui/marketplace/pages/MkManageAssetView";

import type { ManageAsset } from "@data/lib/catalyst/marketplace/index";
import { track } from "@core/lib/telemetry/track";
import type { TrackContext } from "@core/lib/telemetry/track";

type ManageAction = "sell" | "transfer" | "cancel";

export type ManageAssetViewProps = {
  asset: ManageAsset;
  actionHub: boolean;
  trackCtx: TrackContext;
};

function flowHref(action: ManageAction, asset: ManageAsset): string {
  if (action === "sell") return `/marketplace/sell?id=${encodeURIComponent(asset.id)}`;
  if (action === "transfer") {
    return `/marketplace/transfer?id=${encodeURIComponent(asset.id)}`;
  }
  const orderId = asset.order?.id ?? "";
  return `/marketplace/cancel?order=${encodeURIComponent(orderId)}&owner=${encodeURIComponent(asset.ownerAddress)}`;
}

export default function ManageAssetView({
  asset,
  actionHub,
  trackCtx,
}: ManageAssetViewProps) {
  const navigate = useNavigate();
  const [pending, setPending] = useState<ManageAction | null>(null);

  const listed = asset.order != null;
  const locked = asset.rentalActive;

  function onAction(action: ManageAction) {
    track("mk_manage_action_clicked", { item_id: asset.id, action }, trackCtx);
    setPending(action);
    void navigate(flowHref(action, asset));
  }

  const pageAsset = {
    name: asset.name,
    category: asset.category,
    coords: asset.tokenId ? `#${asset.tokenId.slice(-6)}` : asset.rarity,
    network: asset.networkLabel,
    owner: asset.ownerShort,
    image: asset.image ?? null,
    description:
      asset.description ||
      `A ${asset.rarity} ${asset.category} you own. Manage its listing, transfer it, or cancel an active sale.`,
    proximities: [],
  };
  const pageOrder = asset.order
    ? { price: asset.order.price, expiresAt: asset.order.expiresAt }
    : null;

  return (
    <MkManageAssetView
      actionHub={actionHub}
      name={asset.name}
      image={asset.image}
      rarity={asset.rarity}
      listed={listed}
      locked={locked}
      pending={pending}
      orderPrice={asset.order?.price}
      orderExpiresAt={asset.order?.expiresAt}
      pageAsset={pageAsset}
      pageOrder={pageOrder ?? undefined}
      onAction={onAction}
    />
  );
}
