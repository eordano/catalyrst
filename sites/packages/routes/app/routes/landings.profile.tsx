import { useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router";

import StProfileOverviewTab from "@ui/web/pages/StProfileOverviewTab";
import StProfileCreationsTab from "@ui/web/pages/StProfileCreationsTab";
import StProfilePlacesTab from "@ui/web/pages/StProfilePlacesTab";
import StProfilePhotosTab from "@ui/web/pages/StProfilePhotosTab";
import StProfileCommunitiesTab from "@ui/web/pages/StProfileCommunitiesTab";
import StProfileMyAssetsTab from "@ui/web/pages/StProfileMyAssetsTab";

import {
  fetchProfile,
  mapProfile,
  emptyProfile,
  isEthAddress,
  normalizeAddress,
  type ProfileVM,
} from "@data/lib/catalyst/overlay/profile";
import { fetchCreations, type Creations } from "@data/lib/catalyst/marketplace/index";
import {
  fetchOwnedWearables,
  fetchOwnedNames,
  type OwnedItem,
  type OwnedName,
} from "@data/lib/catalyst/marketplace/account";
import {
  fetchMyCommunities,
  type ProfileCommunity,
} from "@data/lib/catalyst/overlay/profile-communities";
import { fetchUserPhotos, type GalleryImage } from "@data/lib/catalyst/overlay/passport";
import { useAuth } from "@data/lib/auth/context";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/landings.profile";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/profile";

const MEMBER_TABS = ["overview", "creations", "communities", "places", "photos"] as const;
const OWN_TABS = ["overview", "assets", "communities", "places", "photos", "referral-rewards"] as const;

type Tab =
  | "overview"
  | "creations"
  | "communities"
  | "places"
  | "photos"
  | "assets"
  | "referral-rewards";

function readTab(params: URLSearchParams, own: boolean): Tab {
  const raw = (params.get("tab")?.trim() ?? "") as Tab;
  const allowed = own ? OWN_TABS : MEMBER_TABS;
  return (allowed as readonly string[]).includes(raw) ? raw : "overview";
}

function tabModel(own: boolean): { id: string; label: string }[] {
  return own
    ? [
        { id: "overview", label: "Overview" },
        { id: "assets", label: "My Assets" },
        { id: "communities", label: "My Communities" },
        { id: "places", label: "My Places" },
        { id: "photos", label: "My Photos" },
        { id: "referral-rewards", label: "Referral Rewards" },
      ]
    : [
        { id: "overview", label: "Overview" },
        { id: "creations", label: "Creations" },
        { id: "communities", label: "Communities" },
        { id: "places", label: "Places" },
        { id: "photos", label: "Photos" },
      ];
}

type PhotoVM = {
  id: string;
  grad: string;
  metadata: {
    userName: string;
    userAddress: string;
    dateTime: string;
    realm: string;
    scene: { name: string; location: { x: string; y: string } };
    visiblePeople: {
      userName: string;
      userAddress: string;
      isGuest: boolean;
      wearables: string[];
    }[];
  };
};

function photoGrad(img: GalleryImage, i: number): string {
  const src = img.thumbnailUrl || img.url;
  if (src) return `#000 url("${src.replace(/"/g, "%22")}") center / cover no-repeat`;
  const h = (i * 47 + 196) % 360;
  return `linear-gradient(150deg, hsl(${h} 62% 48%) 0%, hsl(${(h + 38) % 360} 55% 26%) 100%)`;
}

function mapPhotos(images: GalleryImage[]): PhotoVM[] {
  return images.map((img, i) => {
    const meta = ((img as { metadata?: Record<string, unknown> }).metadata ??
      {}) as Record<string, unknown>;
    const scene = (meta.scene ?? {}) as {
      name?: unknown;
      location?: { x?: unknown; y?: unknown };
    };
    const loc = (scene.location ?? {}) as { x?: unknown; y?: unknown };
    const people = Array.isArray(meta.visiblePeople)
      ? (meta.visiblePeople as Record<string, unknown>[])
      : [];
    return {
      id: img.id,
      grad: photoGrad(img, i),
      metadata: {
        userName: String(meta.userName ?? ""),
        userAddress: String(meta.userAddress ?? ""),
        dateTime: String(meta.dateTime ?? img.dateTime ?? ""),
        realm: String(meta.realm ?? ""),
        scene: {
          name: String(scene.name ?? "Decentraland"),
          location: { x: String(loc.x ?? "0"), y: String(loc.y ?? "0") },
        },
        visiblePeople: people.map((p) => ({
          userName: String(p.userName ?? ""),
          userAddress: String(p.userAddress ?? ""),
          isGuest: Boolean(p.isGuest ?? false),
          wearables: Array.isArray(p.wearables)
            ? (p.wearables as unknown[]).map(String)
            : [],
        })),
      },
    };
  });
}

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "landings_profile",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const pathAddress = (params as Record<string, string | undefined>).address;
  const explicitAddress =
    pathAddress?.trim() ||
    url.searchParams.get("address")?.trim() ||
    url.searchParams.get("wallet")?.trim() ||
    url.searchParams.get("creator")?.trim() ||
    "";
  const cookieWallet = explicitAddress ? "" : readWallet(request) ?? "";
  const rawAddress = explicitAddress || cookieWallet;
  const address = normalizeAddress(rawAddress);

  const own =
    url.searchParams.get("own") === "1" || (!explicitAddress && cookieWallet !== "");
  const tab = readTab(url.searchParams, own);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const wantAssets = tab === "assets";
  const wantCreations = tab === "creations";
  const wantPhotos = tab === "photos";
  const ethAddr = isEthAddress(address);

  const [profileChain, assets, photosRes] = await Promise.all([
    (async (): Promise<{
      liveProfile: ProfileVM | null;
      creations: Creations;
      creationsFallback: boolean;
    }> => {
      let liveProfile: ProfileVM | null = null;
      if (ethAddr) {
        try {
          const avatar = await fetchProfile(address, { signal: request.signal });
          if (avatar) liveProfile = mapProfile(avatar, address);
        } catch {
          liveProfile = null;
        }
      }
      let creations: Creations = { wearables: [], emotes: [] };
      let creationsFallback = !ethAddr;
      if (wantCreations && ethAddr) {
        try {
          creations = await fetchCreations(
            address,
            { first: 48, creatorName: liveProfile?.name },
            { signal: request.signal },
          );
        } catch {
          creations = { wearables: [], emotes: [] };
          creationsFallback = true;
        }
      }
      return { liveProfile, creations, creationsFallback };
    })(),
    (async (): Promise<{ wearables: unknown[]; names: unknown[] }> => {
      if (!wantAssets || !ethAddr) return { wearables: [], names: [] };
      const [ownedW, ownedN] = await Promise.all([
        fetchOwnedWearables(address, { first: 48 }, { signal: request.signal }).catch(
          () => ({ elements: [] as OwnedItem[], total: 0 }),
        ),
        fetchOwnedNames(address, { first: 48 }, { signal: request.signal }).catch(
          () => ({ elements: [] as OwnedName[], total: 0 }),
        ),
      ]);
      return {
        wearables: ownedW.elements.map((w) => ({
          id: w.id,
          name: w.name ?? "Unnamed",
          rarity: w.rarity ?? "common",
          price: w.price ?? "",
          network: w.urn?.includes(":matic:")
            ? "MATIC"
            : w.urn?.includes(":ethereum:")
              ? "ETHEREUM"
              : "MATIC",
          category: w.category ?? "wearable",
          bodyShape: "unisex",
          isSmart: false,
        })),
        names: ownedN.elements.map((n) => ({ id: n.tokenId ?? n.name, stem: n.name })),
      };
    })(),
    (async (): Promise<{ photos: PhotoVM[]; photosFallback: boolean }> => {
      if (!wantPhotos) return { photos: [], photosFallback: false };
      if (!ethAddr) return { photos: [], photosFallback: true };
      try {
        const images = await fetchUserPhotos(address, { signal: request.signal });
        return { photos: mapPhotos(images), photosFallback: false };
      } catch {
        return { photos: [], photosFallback: true };
      }
    })(),
  ]);

  const { liveProfile, creations, creationsFallback } = profileChain;
  const { photos, photosFallback } = photosRes;

  const profileFallback = liveProfile === null;
  const source: "live" | "fallback" = profileFallback ? "fallback" : "live";
  const profile: ProfileVM = liveProfile ?? emptyProfile(address);

  const payload = {
    sid,
    address,
    own,
    tab,
    source,
    profileFallback,
    hasClaimedName: profile.hasClaimedName ?? false,
    profile,
    creations,
    creationsFallback,
    places: [],
    photos,
    photosFallback,
    assets,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  address: string;
  own: boolean;
  tab: Tab;
  source: "live" | "fallback";
  profileFallback: boolean;
  hasClaimedName: boolean;
  profile: ProfileVM;
  creations: Creations;
  creationsFallback: boolean;
  places: unknown[];
  photos: PhotoVM[];
  photosFallback: boolean;
  assets: { wearables: unknown[]; names: unknown[] };
};

export default function LandingsProfile({ loaderData }: Route.ComponentProps) {
  return <ProfileSurface d={loaderData} />;
}

function ProfileSurface({ d }: { d: LoaderData }) {
  const [, setSearchParams] = useSearchParams();

  useProfileViewed(d.sid, d);

  const tabs = tabModel(d.own);
  const ownTabs = d.own;

  function selectTab(tab: Tab) {
    track("profile_tab_changed", { tab }, { sid: d.sid, story: STORY });
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (tab === "overview") next.delete("tab");
        else next.set("tab", tab);
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function onCardClick(itemId: string) {
    track("profile_card_clicked", { tab: d.tab, item_id: itemId }, { sid: d.sid, story: STORY });
  }

  const profile = d.profile as never;

  return (
    <div className="landings-profile">
      <nav className="landings-profile__tabs" aria-label="Profile sections" style={tabsStyle}>
        {tabs.map((t) => {
          const to = tabHref(d.address, t.id as Tab, ownTabs);
          return (
            <Link
              key={t.id}
              to={to}
              prefetch="intent"
              onClick={(e) => {
                e.preventDefault();
                selectTab(t.id as Tab);
              }}
              aria-current={d.tab === t.id ? "page" : undefined}
              style={tabLinkStyle(d.tab === t.id)}
            >
              {t.label}
            </Link>
          );
        })}
        <Link
          to={ownTabs ? profilePath(d.address) : `${profilePath(d.address)}?own=1`}
          prefetch="intent"
          style={modeToggleStyle}
        >
          {ownTabs ? "View as member" : "View as owner"}
        </Link>
      </nav>

      <ActiveTab d={d} profile={profile} tabs={tabs} onCardClick={onCardClick} />
    </div>
  );
}

function ActiveTab({
  d,
  profile,
  tabs,
  onCardClick,
}: {
  d: LoaderData;
  profile: never;
  tabs: { id: string; label: string }[];
  onCardClick: (id: string) => void;
}) {
  switch (d.tab) {
    case "creations":
      return (
        <StProfileCreationsTab
          profile={profile}
          tabs={tabs}
          isOwnProfile={d.own}
          wearables={d.creations.wearables}
          emotes={d.creations.emotes}
        />
      );
    case "places":
      return (
        <StProfilePlacesTab
          profile={profile}
          isOwnProfile={d.own}
          places={d.places as React.ComponentProps<typeof StProfilePlacesTab>["places"]}
        />
      );
    case "photos":
      return (
        <StProfilePhotosTab
          profile={profile}
          isOwnProfile={d.own}
          photos={d.photos as React.ComponentProps<typeof StProfilePhotosTab>["photos"]}
        />
      );
    case "communities":
      return <CommunitiesTab d={d} profile={profile} tabs={tabs} />;
    case "assets":
      return (
        <StProfileMyAssetsTab
          profile={profile}
          tabs={tabs}
          wearables={d.assets.wearables as React.ComponentProps<typeof StProfileMyAssetsTab>["wearables"]}
          names={d.assets.names as React.ComponentProps<typeof StProfileMyAssetsTab>["names"]}
        />
      );
    case "referral-rewards":
    case "overview":
    default:
      return (
        <StProfileOverviewTab
          profile={profile}
          activeTab="overview"
          isOwnProfile={d.own}
        />
      );
  }
}

function CommunitiesTab({
  d,
  profile,
  tabs,
}: {
  d: LoaderData;
  profile: never;
  tabs: { id: string; label: string }[];
}) {
  const { identity } = useAuth();
  const [rows, setRows] = useState<ProfileCommunity[]>([]);
  const [loading, setLoading] = useState<boolean>(d.own);

  useEffect(() => {
    if (!d.own) {
      setRows([]);
      setLoading(false);
      return;
    }
    if (!identity) {
      setRows([]);
      setLoading(false);
      return;
    }

    const ctrl = new AbortController();
    let live = true;
    setLoading(true);
    fetchMyCommunities(identity, { signal: ctrl.signal })
      .then((list) => {
        if (live) setRows(list);
      })
      .catch(() => {
        if (!live) return;
        setRows([]);
        track(
          "profile_communities_load_failed",
          { address: d.address },
          { sid: d.sid, story: STORY },
        );
      })
      .finally(() => {
        if (live) setLoading(false);
      });

    return () => {
      live = false;
      ctrl.abort();
    };
  }, [d.own, d.address, d.sid, identity]);

  return (
    <StProfileCommunitiesTab
      profile={profile}
      tabs={tabs}
      communities={
        rows as React.ComponentProps<typeof StProfileCommunitiesTab>["communities"]
      }
      isOwnProfile={d.own}
      loading={loading}
    />
  );
}

function profilePath(address: string): string {
  return `/profile/${encodeURIComponent(address)}`;
}

function tabHref(address: string, tab: Tab, own: boolean): string {
  const base = profilePath(address);
  const qs = new URLSearchParams();
  if (tab !== "overview") qs.set("tab", tab);
  if (own) qs.set("own", "1");
  const s = qs.toString();
  return s ? `${base}?${s}` : base;
}

const tabsStyle: React.CSSProperties = {
  display: "flex",
  gap: 8,
  alignItems: "center",
  padding: "10px 16px",
  background: "#0d0c11",
  borderBottom: "1px solid rgba(255,255,255,0.08)",
  flexWrap: "wrap",
};

function tabLinkStyle(active: boolean): React.CSSProperties {
  return {
    padding: "6px 14px",
    borderRadius: 999,
    fontSize: 14,
    fontWeight: 600,
    textDecoration: "none",
    color: active ? "#0d0c11" : "rgba(255,255,255,0.78)",
    background: active ? "#ff2d55" : "rgba(255,255,255,0.06)",
  };
}

const modeToggleStyle: React.CSSProperties = {
  padding: "6px 12px",
  borderRadius: 999,
  fontSize: 13,
  fontWeight: 600,
  textDecoration: "none",
  color: "rgba(255,255,255,0.6)",
  border: "1px solid rgba(255,255,255,0.18)",
};

function useProfileViewed(sid: string, d: LoaderData) {
  const last = useRef<string | null>(null);
  useEffect(() => {
    const key = `${d.address}|${d.source}|${d.own}`;
    if (last.current === key) return;
    last.current = key;
    track(
      "profile_viewed",
      {
        address: d.address,
        has_claimed_name: d.hasClaimedName,
        source: d.source,
        own: d.own,
      },
      { sid, story: STORY },
    );
  }, [sid, d.address, d.source, d.own, d.hasClaimedName]);
}
