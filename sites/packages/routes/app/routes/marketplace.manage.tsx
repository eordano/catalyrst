import { useNavigate } from "react-router";
import { useEffect } from "react";

import Button from "@ui/atoms/Button";

import FlowFallback from "@features/components/marketplace/FlowFallback";
import { openSignIn } from "@features/components/auth/signin-store";
import { useAuth } from "@data/lib/auth/index";

import {} from "@ui/marketplace/pages/MkManageAssetView";

import ManageAssetView from "@features/components/marketplace/ManageAssetView";

import {
  fetchOwnedAsset,
  parseItemId,
  toManageAsset,
  type ManageAsset,
} from "@data/lib/catalyst/marketplace/index";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.manage";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/manage-asset";
const EXPERIMENT_KEY = "marketplace_manage_action_hub";

async function resolveAsset(
  id: string | null,
  owner: string | null,
  signal: AbortSignal,
): Promise<ManageAsset | null> {
  if (!owner || !id) return null;
  const parsed = parseItemId(id);
  if (!parsed) return null;
  try {
    const row = await fetchOwnedAsset(owner, parsed.contractAddress, parsed.itemId, {
      signal,
    });
    return row ? toManageAsset(row) : null;
  } catch {
    return null;
  }
}

const FALLBACK: Assignment = {
  variant: "control",
  flags: { actionHub: false },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const id = url.searchParams.get("id");
  const owner =
    url.searchParams.get("owner")?.trim() || readWallet(request) || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const asset = await resolveAsset(id, owner, request.signal);

  track(
    "mk_manage_asset_viewed",
    {
      item_id: asset?.id ?? id ?? null,
      listed: asset?.order != null,
      has_rental: asset?.rentalActive ?? false,
      network: asset?.network ?? null,
    },
    {
      sid,
      story: STORY,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    },
  );

  const payload = { asset, sid, assignment, needsOwner: !owner };

  return wrap(payload);
}

export default function MarketplaceManageRoute({ loaderData }: Route.ComponentProps) {
  const { asset, sid, assignment, needsOwner } = loaderData as {
    asset: ManageAsset | null;
    sid: string;
    assignment: Assignment;
    needsOwner: boolean;
  };

  const actionHub = assignment.flags?.actionHub === true;

  if (!asset) {
    return <ManageFallback needsOwner={needsOwner} />;
  }

  return (
    <ManageAssetView
      asset={asset}
      actionHub={actionHub}
      trackCtx={{
        sid,
        story: STORY,
        variant: assignment.variant,
        experimentKey: assignment.experimentKey,
      }}
    />
  );
}

function ManageFallback({ needsOwner }: { needsOwner: boolean }) {
  const auth = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    if (needsOwner && auth.isConnected && auth.identity) {
      navigate(
        `/marketplace/account?address=${encodeURIComponent(auth.identity.signer)}`,
        { replace: true },
      );
    }
  }, [needsOwner, auth.isConnected, auth.identity, navigate]);

  if (!needsOwner) {
    return (
      <FlowFallback
        title="Asset not found"
        subtitle="You don't own this asset, or it's missing from the catalog."
        primaryHref="/marketplace/account"
        primaryLabel="View your assets"
      />
    );
  }

  return (
    <FlowFallback
      title="Sign in to manage your assets"
      subtitle="Listings, transfers and bids for things you own live here."
      primaryHref="/shop"
      primaryLabel="Browse the shop"
      secondary={
        <Button variant="primary" onClick={() => openSignIn()}>
          Sign in
        </Button>
      }
    />
  );
}
