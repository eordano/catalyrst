import { toTransferAsset, type TransferAsset } from "@data/lib/catalyst/marketplace/transfer";
import { loadOwnedAssets } from "@data/lib/catalyst/marketplace/transfer.server";
import { isInReturnWindow, readLease } from "@data/lib/catalyst/marketplace/escrow-lease";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import TransferWizard from "@features/stories/marketplace/transfer/TransferWizard";
import LeasedAssetsNotice, {
  type LeasedAsset,
} from "@features/components/marketplace/LeasedAssetsNotice";

import type { Route } from "./+types/marketplace.transfer";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/transfer";
const EXPERIMENT_KEY = "mk_transfer_wizard";

const DEFAULT_ADDRESS = "0x1d9aa2025b67f0f21d1603ce521bda7869098f8a";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const address = (
    url.searchParams.get("address")?.trim() ||
    readWallet(request) ||
    DEFAULT_ADDRESS
  ).toLowerCase();
  const initialStep = url.searchParams.get("step")?.trim() || undefined;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    "marketplace/transfer",
    FALLBACK,
  );

  const owned = await loadOwnedAssets(address, { signal: request.signal });

  const assets: TransferAsset[] = [];
  const leased: LeasedAsset[] = [];
  for (const el of owned.elements ?? []) {
    const lease = readLease(el);
    if (lease && isInReturnWindow(lease)) {
      leased.push({ id: el.id, name: el.name ?? "Untitled", lease });
    } else {
      assets.push(toTransferAsset(el));
    }
  }

  const payload = {
    sid,
    assignment,
    assets,
    leased,
    unavailable: owned.source === "unavailable",
    reason: owned.reason,
    address,
    initialStep,
  };

  return wrap(payload);
}

export default function MarketplaceTransferRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  if (d.unavailable) {
    return (
      <main className="mktransfer-route mktransfer-route--unavailable">
        <p role="alert" style={noticeStyle}>
          We couldn&apos;t read what this wallet holds
          {d.reason ? ` (${d.reason})` : ""}. The wizard is hidden because an
          empty list here would say you own nothing to send. Reload in a moment.
        </p>
      </main>
    );
  }

  return (
    <main className="mktransfer-route">
      {d.leased.length > 0 && <LeasedAssetsNotice items={d.leased} />}
      <TransferWizard
        assets={d.assets}
        source="catalyst"
        initialStep={d.initialStep}
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
      />
    </main>
  );
}

const noticeStyle: React.CSSProperties = {
  margin: "24px 16px",
  padding: "16px 18px",
  borderRadius: 12,
  border: "1px solid rgba(255,45,85,0.4)",
  background: "rgba(255,45,85,0.08)",
  color: "rgba(255,255,255,0.86)",
  fontSize: 15,
  lineHeight: 1.5,
  maxWidth: 720,
};
