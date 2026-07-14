import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import CollectionDetailView from "@ui/creatorhub/workflows/CollectionDetailView";
import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import ChNotFound from "@ui/creatorhub/pages/ChNotFound";

import { fetchCollectionItems, fetchCollectionMeta, fetchOnchainCollectionDetail, isContractCollectionId, isSimulatedCollectionId, mergeCollectionMeta, readTab, type CollectionMeta, type EmoteItem, type ItemTab, type WearableItem } from "@data/lib/catalyst/builder/collection-detail";
import { CatalystError } from "@data/lib/catalyst/client";
import { readSimCollectionItems } from "@data/lib/catalyst/builder/sim-collection-items";
import type { ChCollectionItem } from "@ui/creatorhub/pages/ChCollectionDetail";
import { loadCollectionDetail } from "@data/lib/catalyst/builder/collection-detail.server";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { useAuth } from "@data/lib/auth/context";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.wearables.collections_.$id";
import type { StoryId } from "@core/lib/telemetry/story-id";

export function meta({ loaderData }: Route.MetaArgs) {
  const collection = (
    loaderData as { collection?: { name?: string } } | undefined
  )?.collection;
  return creatorHubMeta(collection?.name?.trim() || "Collection");
}

const STORY: StoryId = "creator-hub/wearable-collection-detail";

const FALLBACK: Assignment = {
  variant: "split-detail",
  flags: { splitItems: true },
  experimentKey: "creator_wearable_collection_detail",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const tab = readTab(url.searchParams.get("tab"));

  const id = (params.id ?? "").trim();

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const address =
    url.searchParams.get("address")?.trim().toLowerCase() ||
    readWallet(request) ||
    null;

  const { collection, source, itemCount, fallback, missing } =
    await loadCollectionDetail(id, { signal: request.signal }, address);

  const payload = {
    sid,
    id,
    tab,
    address,
    collection,
    source,
    itemCount,
    fallback,
    missing,
  };

  return wrap(payload);
}

type LiveItems = {
  wearables: WearableItem[];
  emotes: EmoteItem[];
  meta: CollectionMeta | null;
};

export default function CreatorWearableCollectionDetail({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const navigate = useNavigate();
  const [, setSearchParams] = useSearchParams();
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);

  const { items: live, pending, missing: liveMissing } = useLiveItems(
    d.id,
    d.address,
  );

  const notFound = (d.missing && !live) || liveMissing;

  const loading = pending && !live;

  const wearables = live?.wearables ?? d.collection.wearables;
  const emotes = live?.emotes ?? d.collection.emotes;
  const liveCount = wearables.length + emotes.length;
  const effectiveSource: "catalyst" | "empty" =
    live && liveCount > 0 ? "catalyst" : d.source;

  useViewEvent(d.sid, d.id, d.tab, liveCount || d.itemCount, effectiveSource, notFound);

  const collection = live?.meta
    ? mergeCollectionMeta(d.collection, live.meta)
    : d.collection;

  const tabRef = useRef<ItemTab>(d.tab);
  useEffect(() => {
    tabRef.current = d.tab;
  }, [d.tab]);

  function onItemTypeChange(type: "wearable" | "emote") {
    const tab: ItemTab = type === "emote" ? "emotes" : "wearables";
    tabRef.current = tab;
    track("creator_collection_tab_changed", { tab }, { sid: d.sid, story: STORY });
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (tab === "wearables") next.delete("tab");
        else next.set("tab", tab);
        return next;
      },
      { replace: true, preventScrollReset: true },
    );
  }

  function onItemOpen(item: ChCollectionItem) {
    const tab = tabRef.current;
    const kind = tab === "emotes" ? "emote" : "wearable";
    track(
      "creator_collection_item_clicked",
      { id: d.id, itemId: item.id, tab, kind },
      { sid: d.sid, story: STORY },
    );
    navigate(
      `/create/wearables/item-editor?collection=${encodeURIComponent(d.id)}&item=${encodeURIComponent(item.id)}`,
    );
  }

  if (notFound) {
    return (
      <CreatorHubChrome
        active="collections"
        signedIn={isConnected}
        account={address ?? ""}
        name={name}
        onSignIn={() => openSignIn()}
      >
        <ChNotFound
          bare
          subtitle={"This collection isn't available \u{2014} it may belong to another account or have been removed"}
          backLabel="Back to Collections"
          onBack={() => navigate("/create/wearables")}
        />
      </CreatorHubChrome>
    );
  }

  return (
    <CollectionDetailView
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => openSignIn()}
      loading={loading}
      collection={collection}
      wearables={wearables}
      emotes={emotes}
      initialItemType={d.tab === "emotes" ? "emote" : "wearable"}
      onBack={() => navigate("/create/wearables")}
      onItemTypeChange={onItemTypeChange}
      onItemOpen={onItemOpen}
      onPublish={() =>
        navigate(`/create/wearables/publish?collection=${encodeURIComponent(d.id)}`)
      }
      onAddItems={() =>
        navigate(`/create/wearables/item-editor?collection=${encodeURIComponent(d.id)}`)
      }
    />
  );
}

function useLiveItems(
  id: string,
  loaderAddress: string | null,
): {
  items: LiveItems | null;
  pending: boolean;
  missing: boolean;
} {
  const auth = useAuth();
  const connected = auth.isConnected;
  const authAddress = auth.address;
  const authFetch = auth.fetch;
  const [live, setLive] = useState<LiveItems | null>(null);
  const [pending, setPending] = useState(false);
  const [missing, setMissing] = useState(false);

  useEffect(() => {
    if (!id || isSimulatedCollectionId(id)) {
      const sim = id ? readSimCollectionItems(id) : null;
      setLive(sim ? { ...sim, meta: null } : null);
      setPending(false);
      setMissing(false);
      return;
    }

    if (isContractCollectionId(id)) {
      const address = (authAddress ?? loaderAddress ?? "").toLowerCase();
      if (!address) {
        setLive(null);
        setPending(false);
        setMissing(false);
        return;
      }
      let cancelled = false;
      setPending(true);
      setMissing(false);
      fetchOnchainCollectionDetail(address, id, { base: "" })
        .then((res) => {
          if (cancelled) return;
          if (!res.found) {
            setLive(null);
            setMissing(true);
            return;
          }
          setLive({
            wearables: res.wearables,
            emotes: res.emotes,
            meta: res.meta,
          });
        })
        .catch(() => {
          if (!cancelled) setLive(null);
        })
        .finally(() => {
          if (!cancelled) setPending(false);
        });
      return () => {
        cancelled = true;
      };
    }

    if (!connected) {
      setLive(null);
      setPending(false);
      setMissing(false);
      return;
    }
    let cancelled = false;
    setPending(true);
    setMissing(false);
    const fetchImpl: typeof fetch = (input, init) =>
      authFetch(typeof input === "string" ? input : input.toString(), init);
    const opts = { fetchImpl, base: "" };
    Promise.all([
      fetchCollectionItems(id, opts),
      fetchCollectionMeta(id, opts).catch(() => null),
    ])
      .then(([items, meta]) => {
        if (!cancelled) setLive({ ...items, meta });
      })
      .catch(async (err: unknown) => {
        if (cancelled) return;
        setLive(null);
        if (err instanceof CatalystError && err.status === 404) {
          const gone = await confirmCollectionGone(authFetch, id);
          if (!cancelled && gone) setMissing(true);
        }
      })
      .finally(() => {
        if (!cancelled) setPending(false);
      });
    return () => {
      cancelled = true;
    };
  }, [connected, authFetch, authAddress, loaderAddress, id]);

  return { items: live, pending, missing };
}

type UseAuthFetch = ReturnType<typeof useAuth>["fetch"];

async function confirmCollectionGone(
  authFetch: UseAuthFetch,
  id: string,
): Promise<boolean> {
  try {
    const res = await authFetch(
      `/v1/collections/${encodeURIComponent(id)}`,
      { metadata: {} },
    );
    if (res.status !== 404) return false;
    const body: unknown = await res.json();
    return (
      typeof body === "object" && body !== null &&
      (body as { ok?: unknown }).ok === false
    );
  } catch {
    return false;
  }
}

function useViewEvent(
  sid: string,
  id: string,
  tab: ItemTab,
  itemCount: number,
  source: "catalyst" | "empty",
  missing: boolean,
) {
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${id}|${source}|${missing}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    track(
      "creator_collection_detail_viewed",
      { id, tab, itemCount, source, missing },
      { sid, story: STORY },
    );
  }, [sid, id, tab, itemCount, source, missing]);
}
