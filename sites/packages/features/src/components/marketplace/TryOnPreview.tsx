import MkTryOnPreview from "@ui/marketplace/components/MkTryOnPreview";

import { parseItemRef } from "@data/lib/catalyst/marketplace/cart";

const NON_WEARABLE = new Set([
  "emote",
  "ens",
  "name",
  "parcel",
  "estate",
  "land",
]);

function itemUrnFor(itemId: string, network?: string | null): string | null {
  if (network === "ethereum") return null;
  const ref = parseItemRef(itemId);
  if (!ref) return null;
  return `urn:decentraland:matic:collections-v2:${ref.collection}:${ref.itemId}`;
}

export function canTryOn(
  network?: string | null,
  category?: string | null,
  kind?: "wearable" | "emote",
): boolean {
  if (!network) return false;
  if (network === "ethereum") return false;
  if (kind === "emote") return false;
  return !NON_WEARABLE.has((category ?? "").toLowerCase());
}

export default function TryOnPreview({
  itemId,
  network,
}: {
  itemId: string;
  network?: string | null;
}) {
  const urn = itemUrnFor(itemId, network);
  if (!urn) return null;
  return <MkTryOnPreview urn={urn} />;
}
