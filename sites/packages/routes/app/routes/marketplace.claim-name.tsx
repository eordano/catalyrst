import { useMemo, useRef } from "react";
import { useAuth } from "@data/lib/auth/context";
import { walletProvider } from "@data/lib/auth/wallet";
import { nameClaimWallet } from "@data/lib/catalyst/marketplace/name-claim";
import { Link, redirect, useNavigate, useSearchParams } from "react-router";

import MkFlowBanner from "@ui/marketplace/components/MkFlowBanner";

import {
  fetchOwnedNames,
  NAME_REGEX,
} from "@data/lib/catalyst/marketplace/names";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoaderWith } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import ClaimNameWizard from "@features/stories/marketplace/claim-name/ClaimNameWizard";

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

  const { sid, assignment, wrap, data: ownedNames } = await storyLoaderWith(
    request,
    STORY,
    FALLBACK,
    () =>
      owner
        ? fetchOwnedNames(owner, { signal: request.signal })
            .then((page) => page.elements.map((e) => e.name))
            .catch(() => [] as string[])
        : Promise.resolve([] as string[]),
  );

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
  const auth = useAuth();
  const authRef = useRef(auth);
  authRef.current = auth;
  const claim = useMemo(() => nameClaimWallet(() => {
    if (!authRef.current.address || !authRef.current.isConnected) throw new Error("Sign in with your wallet to register a NAME.");
    return { provider: walletProvider(), address: authRef.current.address };
  }), [auth.address]);
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


  return (
    <main className="marketplace-claim-name" onClickCapture={carryDeployContext}>
      {fromDeploy ? (
        <div
          role="navigation"
          aria-label="Back to publishing"
          style={{
            display: "flex",
            alignItems: "center",
            padding: "12px 20px",
            background: "var(--panel, #16121c)",
            borderBottom: "1px solid var(--line, rgba(255,255,255,0.18))",
          }}
        >
          <Link
            to={returnToPublishUrl()}
            style={{
              display: "inline-flex",
              alignItems: "center",
              minHeight: 44,
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
        key={auth.address ?? "anonymous"}
        allowStepPreview={false}
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        takenNames={takenNames}
        sampleName={sampleName}
        check={claim.check}
        approve={claim.approve}
        mint={claim.mint}
        initialStep={step ?? undefined}
        banner={
          <MkFlowBanner>
            Register your NAME with MANA on Ethereum. Your wallet will confirm approval, registration and gas fees.
          </MkFlowBanner>
        }
        creditsNote={"Credits can't be used for NAMEs yet \u{2014} Credits checkout only supports collection items."}
        onReturnToPublish={
          fromDeploy
            ? (worldName) => {
                track(
                  "ch_claim_name_returned_to_publish",
                  { name: worldName, simulated: false },
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
