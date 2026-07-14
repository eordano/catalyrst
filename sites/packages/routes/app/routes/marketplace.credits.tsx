import React, { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import NewShopTabs from "@ui/marketplace/new-shop/NewShopTabs";
import MkSuccessPage from "@ui/marketplace/pages/MkSuccessPage";
import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/marketplace/new-shop/newshoptabs.css";

import CreditsHub from "@features/components/marketplace/CreditsHub";
import ClaimCaptchaModal from "@features/components/marketplace/ClaimCaptchaModal";
import { fetchProgressSigned, seasonsToShellVM, toHubVM, type CreditsHubVM } from "@data/lib/catalyst/marketplace/credits";
import { loadSeasons } from "@data/lib/catalyst/marketplace/credits.server";
import { useAuth } from "@data/lib/auth/context";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { openSignIn } from "@features/components/auth/signin-store";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { marketplaceMeta } from "@core/lib/seo/marketplace-meta";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.credits";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => marketplaceMeta("Credits");

const STORY: StoryId = "marketplace/credits";

const SHOP_TABS = [
  { id: "overview", label: "Overview", href: "/shop" },
  { id: "all-assets", label: "All Assets", href: "/shop?tab=all-assets" },
  { id: "names", label: "NAMEs", href: "/marketplace/names" },
  { id: "my-assets", label: "My Assets", href: "/marketplace/account" },
  { id: "my-favorites", label: "My Favorites", href: "/shop?tab=my-favorites" },
  { id: "cart", label: "Cart", href: "/marketplace/cart" },
] as const;

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "marketplace_credits",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const wallet =
    url.searchParams.get("wallet")?.trim().toLowerCase() ||
    readWallet(request) ||
    undefined;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const seasons = await loadSeasons(request.signal);

  const payload = {
    sid,
    wallet: wallet ?? null,
    seasons,
  };

  return wrap(payload);
}

export default function MarketplaceCreditsRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const auth = useAuth();

  const [hub, setHub] = useState<CreditsHubVM | null>(
    d.seasons ? seasonsToShellVM(d.seasons) : null,
  );

  useEffect(() => {
    setHub(d.seasons ? seasonsToShellVM(d.seasons) : null);
  }, [d.seasons]);

  useEffect(() => {
    if (d.wallet || !auth.address) return;
    const me = auth.address;
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("wallet", me);
        return next;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [d.wallet, auth.address, setSearchParams]);

  const [progressEpoch, setProgressEpoch] = useState(0);
  const [progressError, setProgressError] = useState(false);
  useEffect(() => {
    const seasons = d.seasons;
    if (!seasons || !auth.isConnected || !auth.identity || !auth.address) return;
    const ctrl = new AbortController();
    fetchProgressSigned(auth.identity, auth.address, { signal: ctrl.signal })
      .then((progress) => {
        setHub(toHubVM(seasons, progress));
        setProgressError(false);
      })
      .catch(() => {
        if (!ctrl.signal.aborted) setProgressError(true);
      });
    return () => ctrl.abort();
  }, [d.seasons, auth.isConnected, auth.identity, auth.address, progressEpoch]);

  useHubViewed(d.sid, hub);

  const step = searchParams.get("step");
  const claimed = step === "claimed";
  const [claiming, setClaiming] = useState(false);

  function onClaim() {
    if (!auth.isConnected || !auth.identity) {
      openSignIn();
      return;
    }
    setClaiming(true);
  }

  function onClaimSuccess(granted: number) {
    setClaiming(false);
    setProgressEpoch((n) => n + 1);
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("step", "claimed");
        next.set("granted", String(granted));
        return next;
      },
      { preventScrollReset: true },
    );
  }

  if (claimed) {
    const granted = Number(searchParams.get("granted") ?? 0);
    return (
      <MkSuccessPage
        state="success"
        asset={{
          category: "item",
          name: granted > 0
            ? `${granted} Marketplace Credits`
            : "Marketplace Credits",
          rarity: "epic",
        }}
      />
    );
  }

  return (
    <ChromeShell
      className="mk"
      ariaLabel="Credits"
      topbar={<DclTopBar variant="sites" active="shop" />}
      subnav={false}
    >
      <NewShopTabs tabs={SHOP_TABS} />
      <main className="mk-credits-route">
        {!auth.isConnected && (
          <div role="status" style={emptyBannerStyle}>
            <strong>Sign in to claim Marketplace Credits.</strong>
            <span style={{ display: "block", marginTop: 4 }}>
              Connect a wallet or continue with email to track the current season
              and your goals.
            </span>
            <button type="button" onClick={() => openSignIn()} style={signInBtnStyle}>
              Sign in
            </button>
          </div>
        )}
        {hub ? (
          <>
            <CreditsHub
              sid={d.sid}
              hub={hub}
              progressUnavailable={auth.isConnected && progressError}
              onRetryProgress={() => setProgressEpoch((n) => n + 1)}
              onClaim={onClaim}
              onClose={() => navigate("/shop")}
            />
            {claiming && auth.identity && (
              <ClaimCaptchaModal
                identity={auth.identity}
                claimable={hub.claimable}
                onSuccess={onClaimSuccess}
                onClose={() => setClaiming(false)}
              />
            )}
          </>
        ) : (
          <p style={{ padding: 24, color: "rgba(255,255,255,0.7)" }}>
            Credits are unavailable right now.
          </p>
        )}
      </main>
    </ChromeShell>
  );
}

const emptyBannerStyle: React.CSSProperties = {
  margin: "12px 16px",
  padding: "12px 16px",
  borderRadius: 12,
  background: "rgba(255,255,255,0.06)",
  border: "1px solid rgba(255,255,255,0.10)",
  color: "rgba(255,255,255,0.82)",
  fontSize: 14,
  lineHeight: 1.5,
};

const signInBtnStyle: React.CSSProperties = {
  marginTop: 12,
  padding: "10px 22px",
  borderRadius: 999,
  border: 0,
  background: "var(--brand-cta)",
  color: "#fff",
  fontSize: 14,
  fontWeight: 600,
  cursor: "pointer",
};

function useHubViewed(sid: string, hub: CreditsHubVM | null) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current || !hub) return;
    fired.current = true;
    track(
      "mk_credits_viewed",
      {
        has_started: hub.hasStartedProgram,
        goal_count: hub.goals.length,
        week: hub.weekNumber,
      },
      { sid, story: STORY },
    );
  }, [sid, hub]);
}
