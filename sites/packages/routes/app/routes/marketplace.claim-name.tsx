import { Link, redirect, useNavigate, useSearchParams } from "react-router";

import MkFlowBanner from "@ui/marketplace/components/MkFlowBanner";

import {
  checkNameAvailability,
  fetchOwnedNames,
  NAME_REGEX,
} from "@data/lib/catalyst/marketplace/names";
import {
  hasWallet,
  getConnectedAddress,
  connectWallet,
  getChainId,
} from "@data/lib/auth/wallet";
import { signTypedData } from "@data/lib/auth/typed-data";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { prepareNameClaim } from "@data/lib/catalyst/marketplace/tx";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import ClaimNameWizard from "@features/stories/marketplace/claim-name/ClaimNameWizard";
import type {
  CheckAvailabilityFn,
  MintFn,
} from "@features/stories/marketplace/claim-name/machine";

import type { Route } from "./+types/marketplace.claim-name";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/claim-name";
const EXPERIMENT_KEY = "mk_claim_name_wizard";

function buildTakenNames(owned: string[]): string[] {
  return [...new Set(owned.map((n) => n.toLowerCase()))];
}

const CLAIM_INTENT = ["name", "step", "from", "project", "world", "origin", "owner"];

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const hasClaimIntent = CLAIM_INTENT.some(
    (k) => (url.searchParams.get(k) ?? "").trim() !== "",
  );
  if (!hasClaimIntent) {
    return redirect("/marketplace/names");
  }

  const step = url.searchParams.get("step")?.trim() || null;
  const owner = url.searchParams.get("owner")?.trim() || readWallet(request) || null;
  const rawName = url.searchParams.get("name")?.trim() ?? "";
  const sampleName = NAME_REGEX.test(rawName) ? rawName : "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let ownedNames: string[] = [];
  if (owner) {
    try {
      const page = await fetchOwnedNames(owner, { signal: request.signal });
      ownedNames = page.elements.map((e) => e.name);
    } catch {
    }
  }

  const payload = {
    sid,
    step,
    assignment,
    takenNames: buildTakenNames(ownedNames),
    sampleName,
    from: url.searchParams.get("from")?.trim() || null,
  };

  return wrap(payload);
}

const DEPLOY_CONTEXT_KEYS = ["project", "world", "origin"] as const;

export default function MarketplaceClaimName({ loaderData }: Route.ComponentProps) {
  const { sid, step, assignment, takenNames, sampleName, from } =
    loaderData;
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();

  const fromDeploy = from === "deploy-world";

  const returnToPublishUrl = (worldName?: string): string => {
    const params = new URLSearchParams();
    if (worldName) {
      params.set("name", worldName);
      params.set("claimed", "1");
    }
    for (const key of DEPLOY_CONTEXT_KEYS) {
      const value = searchParams.get(key)?.trim();
      if (!value) continue;
      params.set(key === "origin" ? "from" : key, value);
    }
    const qs = params.toString();
    return `/creator-hub/deploy-world${qs ? `?${qs}` : ""}`;
  };

  const carryDeployContext = (e: React.MouseEvent<HTMLElement>) => {
    if (!fromDeploy) return;
    const anchor = (e.target as HTMLElement).closest?.("a[href^='/marketplace/names']");
    const href = anchor?.getAttribute("href");
    if (!href) return;
    e.preventDefault();
    const url = new URL(href, window.location.origin);
    url.searchParams.set("from", "deploy-world");
    for (const key of DEPLOY_CONTEXT_KEYS) {
      const value = searchParams.get(key)?.trim();
      if (value) url.searchParams.set(key, value);
    }
    navigate(url.pathname + url.search);
  };

  const realCheck: CheckAvailabilityFn = async ({ name, signal }) => {
    const res = await checkNameAvailability(name, { signal });
    return { available: res.kind === "claimable" };
  };

  const realMint: MintFn = async ({ name }) => {
    if (!hasWallet())
      throw new Error(
        "No browser wallet found. Install MetaMask (or another EIP-1193 wallet).",
      );
    const from = (await getConnectedAddress()) ?? (await connectWallet());
    const chainId = await getChainId();
    const { typedData } = prepareNameClaim({ chainId, beneficiary: from, name });
    const sig = await signTypedData(typedData, from);
    return { txHash: "", tokenId: BigInt("0x" + sig.slice(2, 18)).toString() };
  };

  return (
    <main className="marketplace-claim-name" onClickCapture={carryDeployContext}>
      {fromDeploy ? (
        <div
          role="navigation"
          aria-label="Back to publishing"
          style={{
            position: "fixed",
            left: 0,
            right: 0,
            bottom: 0,
            zIndex: 1000,
            display: "flex",
            alignItems: "center",
            padding: "12px 20px",
            background: "var(--panel, #16121c)",
            borderTop: "1px solid var(--line, rgba(255,255,255,0.18))",
            boxShadow: "0 -6px 18px rgba(0,0,0,0.4)",
          }}
        >
          <Link
            to={returnToPublishUrl()}
            style={{
              color: "var(--brand, #ff2d55)",
              textDecoration: "none",
              fontSize: 14,
              fontWeight: 600,
            }}
          >
            {"\u{2190}"} Back to publishing your scene
          </Link>
        </div>
      ) : null}
      <ClaimNameWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        takenNames={takenNames}
        sampleName={sampleName}
        check={realCheck}
        mint={realMint}
        initialStep={step ?? undefined}
        banner={
          <MkFlowBanner>
            <strong>Test mode {"\u{2014}"} no real purchase will occur.</strong> NAME
            registration isn&apos;t connected on this marketplace yet. You can
            try the flow, and your wallet may ask for a signature, but no NAME
            will be minted and nothing will be charged.
          </MkFlowBanner>
        }
        creditsNote={"Credits can't be used for NAMEs yet \u{2014} Credits checkout only supports collection items."}
        onReturnToPublish={
          fromDeploy
            ? (worldName) => {
                track(
                  "ch_claim_name_returned_to_publish",
                  { name: worldName, simulated: true },
                  { sid, story: "creator-hub/claim-name" },
                );
                navigate(returnToPublishUrl(worldName));
              }
            : undefined
        }
      />
    </main>
  );
}
