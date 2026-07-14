import path from "node:path";

import { useMemo } from "react";

import {
  buildCreateOrder,
  fetchOwnedWearables,
  type CreateOrderFn,
  type OwnedAsset,
} from "@data/lib/catalyst/marketplace/sell";
import { getConnectedAddress, hasWallet, walletProvider } from "@data/lib/auth/wallet";
import { useAuth } from "@data/lib/auth/context";
import { isInReturnWindow, readLease } from "@data/lib/catalyst/marketplace/escrow-lease";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { withCreatorFunnel } from "@core/lib/telemetry/creator-funnel";
import SellWizard from "@features/stories/marketplace/sell-list/SellWizard";
import LeasedAssetsNotice, {
  type LeasedAsset,
} from "@features/components/marketplace/LeasedAssetsNotice";

import type { Route } from "./+types/marketplace.sell";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/sell-list";
const EXPERIMENT_KEY = "marketplace_sell_wizard";
const FIXTURE_PATH = path.join(process.cwd(), "packages", "data", "src", "fixtures", `${STORY}.json`);

async function loadFixtureAssets(): Promise<{ address: string; assets: OwnedAsset[] }> {
  try {
    const fs = await import("node:fs/promises");
    const raw = await fs.readFile(FIXTURE_PATH, "utf8");
    const json = JSON.parse(raw) as { address?: string; elements?: OwnedAsset[] };
    return {
      address: json.address ?? "0x0000000000000000000000000000000000000000",
      assets: Array.isArray(json.elements) ? json.elements : [],
    };
  } catch {
    return { address: "0x0000000000000000000000000000000000000000", assets: [] };
  }
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true, requireReconfirm: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const addressOverride =
    url.searchParams.get("wallet")?.trim() ||
    url.searchParams.get("address")?.trim() ||
    url.searchParams.get("creator")?.trim() ||
    readWallet(request) ||
    null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const address = addressOverride ?? (await loadFixtureAssets()).address;
  let assets: OwnedAsset[] = [];
  try {
    assets = await fetchOwnedWearables(address, { first: 24 }, { signal: request.signal });
  } catch {
    assets = [];
  }

  const sellable: OwnedAsset[] = [];
  const leased: LeasedAsset[] = [];
  for (const a of assets) {
    const lease = readLease(a);
    if (lease && isInReturnWindow(lease)) {
      leased.push({ id: a.id, name: a.name ?? "Untitled", lease });
    } else {
      sellable.push(a);
    }
  }
  assets = sellable;

  const payload = { sid, step, assets, leased, assignment };
  return wrap(payload);
}

export default function MarketplaceSell({ loaderData }: Route.ComponentProps) {
  const { sid, step, assets, leased, assignment } = loaderData;

  const { identity } = useAuth();
  const createOrder = useMemo<CreateOrderFn>(
    () => async (args) => {
      const provider = hasWallet() ? walletProvider() : null;
      const address = provider ? await getConnectedAddress() : null;
      return buildCreateOrder({ identity, provider, address })(args);
    },
    [identity],
  );

  return (
    <main className="marketplace-sell">
      {leased.length > 0 && <LeasedAssetsNotice items={leased} />}
      <SellWizard
        createOrder={createOrder}
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        track={withCreatorFunnel(undefined, { sid })}
        assets={assets}
        initialStep={step ?? undefined}
      />
    </main>
  );
}
