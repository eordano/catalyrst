import { useEffect, useRef, useState } from "react";
import { redirect, useNavigate, useNavigation, useSearchParams } from "react-router";

import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import NewShopTabs from "@ui/marketplace/new-shop/NewShopTabs";
import MkNamesPage, { type MkNameStatus } from "@ui/marketplace/pages/MkNamesPage";
import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/marketplace/new-shop/newshoptabs.css";
import "@ui/marketplace/pages/mknamespage.css";
import "@ui/marketplace/components/enscard.css";

import { openSignIn } from "@features/components/auth/signin-store";
import { useAuth } from "@data/lib/auth/index";
import {
  checkNameAvailability,
  classifyName,
  NAME_ECONOMICS,
  type NameAvailability,
} from "@data/lib/catalyst/marketplace/names";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.names";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/names";
const NO_TAKEN: ReadonlySet<string> = new Set();

const SHOP_TABS = [
  { id: "overview", label: "Overview", href: "/shop" },
  { id: "all-assets", label: "All Assets", href: "/shop?tab=all-assets" },
  { id: "names", label: "NAMEs", href: "/marketplace/names" },
  { id: "my-assets", label: "My Assets", href: "/marketplace/account" },
  { id: "my-favorites", label: "My Favorites", href: "/shop?tab=my-favorites" },
  { id: "cart", label: "Cart", href: "/marketplace/cart" },
] as const;

const CREDITS_NOTE =
  "Credits can't be used for NAMEs yet \u{2014} Credits checkout only supports collection items.";
const CHECK_ERROR = "Couldn't check availability right now. Please try again.";

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "marketplace_names",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  if (url.searchParams.get("step") === "claim") {
    const carried = new URLSearchParams(url.searchParams);
    carried.delete("step");
    const qs = carried.toString();
    return redirect(`/marketplace/claim-name${qs ? `?${qs}` : ""}`, 308);
  }

  const query = (
    url.searchParams.get("search") ??
    url.searchParams.get("name") ??
    ""
  ).trim();

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let result: NameAvailability | null = null;
  let checkFailed = false;
  if (query && classifyName(query, NO_TAKEN).kind === "available") {
    try {
      result = await checkNameAvailability(query, { signal: request.signal });
    } catch {
      checkFailed = true;
    }
  }

  const payload = { sid, query, result, checkFailed };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  query: string;
  result: NameAvailability | null;
  checkFailed: boolean;
};

function deriveStatus(
  value: string,
  d: LoaderData,
  navigating: boolean,
): MkNameStatus {
  const trimmed = value.trim();
  if (!trimmed) return { kind: "idle" };

  const cls = classifyName(trimmed, NO_TAKEN);
  if (cls.kind === "invalid")
    return { kind: "invalid", message: cls.message, warn: cls.warn };

  if (trimmed !== d.query || navigating) return { kind: "checking" };
  if (d.checkFailed) return { kind: "error", message: CHECK_ERROR };

  if (d.result?.kind === "claimable")
    return { kind: "claimable", priceMana: NAME_ECONOMICS.priceMana };
  if (d.result?.kind === "listed")
    return { kind: "listed", name: d.result.name, priceMana: d.result.priceMana };
  if (d.result?.kind === "taken") return { kind: "taken", name: d.result.name };

  return { kind: "checking" };
}

export default function MarketplaceNames({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const [searchParams, setSearchParams] = useSearchParams();
  const navigation = useNavigation();
  const navigate = useNavigate();
  const auth = useAuth();

  const [value, setValue] = useState(d.query);

  useEffect(() => {
    const trimmed = value.trim();
    if (trimmed === d.query) return;
    const t = setTimeout(() => {
      setSearchParams(
        (prev) => {
          const p = new URLSearchParams(prev);
          if (trimmed) p.set("search", trimmed);
          else p.delete("search");
          p.delete("name");
          return p;
        },
        { replace: true, preventScrollReset: true },
      );
    }, 350);
    return () => clearTimeout(t);
  }, [value, d.query, setSearchParams]);

  useNameChecked(d);

  const status = deriveStatus(value, d, navigation.state !== "idle");

  function gateSignIn(action: string): boolean {
    if (auth.isConnected) return true;
    track(
      "mk_names_signin_gated",
      { action, query: d.query },
      { sid: d.sid, story: STORY },
    );
    openSignIn();
    return false;
  }

  function onClaim() {
    if (d.result?.kind !== "claimable") return;
    track("mk_names_claim_clicked", { name: d.query }, { sid: d.sid, story: STORY });
    if (!gateSignIn("claim")) return;
    const params = new URLSearchParams();
    params.set("name", d.query);
    for (const key of ["from", "project", "world", "origin"]) {
      const value = searchParams.get(key)?.trim();
      if (value) params.set(key, value);
    }
    navigate(`/marketplace/claim-name?${params.toString()}`);
  }

  function onBuy() {
    if (d.result?.kind !== "listed") return;
    const r = d.result;
    track(
      "mk_names_buy_clicked",
      { name: r.name, price_wei: r.priceWei },
      { sid: d.sid, story: STORY },
    );
    if (!gateSignIn("buy")) return;
    navigate(
      `/marketplace/buy?nft=${encodeURIComponent(`${r.contractAddress}-${r.tokenId}`)}`,
    );
  }

  function onTab(id: string) {
    track("mk_names_tab", { tab: id }, { sid: d.sid, story: STORY });
  }

  return (
    <ChromeShell
      className="mk"
      ariaLabel="NAMEs"
      topbar={<DclTopBar variant="sites" active="shop" />}
      subnav={false}
    >
      <div className="mknamespage__subnav">
        <NewShopTabs tabs={SHOP_TABS} active="names" onTab={onTab} />
      </div>
      <MkNamesPage
        chrome={false}
        value={value}
        status={status}
        maxLength={NAME_ECONOMICS.maxNameSize}
        creditsNote={
          status.kind === "claimable" || status.kind === "listed"
            ? CREDITS_NOTE
            : undefined
        }
        onChange={setValue}
        onTab={onTab}
        onClaim={onClaim}
        onBuy={onBuy}
      />
    </ChromeShell>
  );
}

function useNameChecked(d: LoaderData) {
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    if (!d.query) return;
    const key = `${d.query}:${d.result?.kind ?? (d.checkFailed ? "error" : "none")}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    track(
      "mk_names_checked",
      { query: d.query, result: d.result?.kind ?? (d.checkFailed ? "error" : "invalid") },
      { sid: d.sid, story: STORY },
    );
  }, [d]);
}
