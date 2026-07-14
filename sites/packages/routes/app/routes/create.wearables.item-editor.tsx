import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import {
  CATEGORIES,
  RARITIES,
} from "@data/lib/catalyst/builder/item-editor";
import {
  fetchCollections,
  type BuilderCollection,
} from "@data/lib/catalyst/builder/collections";
import { fetchCollectionItems } from "@data/lib/catalyst/builder/collection-detail";
import { useAuth } from "@data/lib/auth/context";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import WearableItemEditorWizard, {
  type CollectionOption,
} from "@features/stories/creator-hub/wearable-item-editor/WearableItemEditorWizard";
import type { WearableDraft } from "@features/stories/creator-hub/wearable-item-editor/machine";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.wearables.item-editor";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Item Editor");

const STORY: StoryId = "creator-hub/wearable-item-editor";
const EXPERIMENT_KEY = "bd_wearable_item_editor";

function collectionStatus(c: BuilderCollection): string {
  if (c.status === "under_review" || c.pending) return "under_review";
  if (c.is_published) return "published";
  return "draft";
}

function toCollectionOptions(collections: BuilderCollection[]): CollectionOption[] {
  return collections.map((c) => ({
    id: c.id,
    name: c.name,
    items: [],
    itemCount: c.count,
    status: collectionStatus(c),
  }));
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const collectionId = url.searchParams.get("collection")?.trim() || null;
  const itemId = url.searchParams.get("item")?.trim() || null;
  const address =
    url.searchParams.get("address")?.trim().toLowerCase() ||
    readWallet(request) ||
    "";
  const needsConnect = address === "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let collections: BuilderCollection[] = [];
  let error = false;
  if (!needsConnect) {
    try {
      collections = await fetchCollections(address, { signal: request.signal });
    } catch {
      error = true;
    }
  }
  const fallback = !needsConnect && (error || collections.length === 0);

  const collectionOptions = toCollectionOptions(collections);
  const selectedId =
    collectionId && collections.some((c) => c.id === collectionId)
      ? collectionId
      : (collections[0]?.id ?? "");

  const draft: WearableDraft = {
    collectionId: selectedId,
    itemId: itemId ?? "",
    name: "",
    modelFile: "",
    category: "upper_body",
    rarity: "common",
    price: "",
    free: true,
  };

  const payload = {
    sid,
    step,
    address,
    draft,
    collections: collectionOptions,
    categories: CATEGORIES as readonly string[],
    rarities: RARITIES as readonly string[],
    needsConnect,
    fallback,
    error,
    assignment,
  };
  return wrap(payload);
}

function useCollectionItems(
  collections: CollectionOption[],
  collectionId: string,
): { collections: CollectionOption[]; itemsError: boolean } {
  const auth = useAuth();
  const connected = auth.isConnected;
  const authFetch = auth.fetch;
  const [items, setItems] = useState<CollectionOption["items"] | null>(null);
  const [itemsError, setItemsError] = useState(false);

  useEffect(() => {
    if (!connected || !collectionId) {
      setItems(null);
      setItemsError(false);
      return;
    }
    let cancelled = false;
    const fetchImpl: typeof fetch = (input, init) =>
      authFetch(typeof input === "string" ? input : input.toString(), init);
    const opts = { fetchImpl, base: "" };
    fetchCollectionItems(collectionId, opts)
      .then(({ wearables, emotes }) => {
        if (cancelled) return;
        setItemsError(false);
        setItems([
          ...wearables.map((w) => ({
            id: w.id,
            name: w.name,
            type: "wearable" as const,
          })),
          ...emotes.map((e) => ({
            id: e.id,
            name: e.name,
            type: "emote" as const,
          })),
        ]);
      })
      .catch(() => {
        if (cancelled) return;
        setItems(null);
        setItemsError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [connected, authFetch, collectionId]);

  const merged = useMemo(() => {
    if (!items) return collections;
    return collections.map((c) =>
      c.id === collectionId
        ? { ...c, items, itemsLoaded: true, itemCount: items.length }
        : c,
    );
  }, [collections, items, collectionId]);

  return { collections: merged, itemsError };
}

export default function CreateWearableItemEditorRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { collections, itemsError } = useCollectionItems(
    d.collections,
    d.draft.collectionId,
  );

  const [, setSearchParams] = useSearchParams();
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);
  useEffect(() => {
    if (isConnected && address && d.address === "") {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("address", address);
          return next;
        },
        { replace: true, preventScrollReset: true },
      );
    }
  }, [isConnected, address, d.address, setSearchParams]);

  return (
    <CreatorHubChrome
      active="collections"
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => {
        openSignIn();
      }}
    >
      <CreatorHubBreadcrumb
        to={href("/create/wearables")}
        label="Collections"
        LinkComponent={Link}
      />

      <main className="create-wearable-item-editor">
        {d.needsConnect ? (
          <div role="status" style={CONNECT_NOTICE_STYLE}>
            Sign in to pick a collection and list its items. You can
            still draft a brand-new wearable below.
          </div>
        ) : d.error ? (
          <div role="alert" style={ALERT_STYLE}>
            Couldn&#x2019;t load Builder collections for this address &#x2014; showing an empty
            picker. You can still draft a brand-new wearable below.
          </div>
        ) : d.fallback ? (
          <div role="status" style={CONNECT_NOTICE_STYLE}>
            No Builder collections found for this address yet &#x2014; the picker is
            empty. You can still draft a brand-new wearable below.
          </div>
        ) : itemsError ? (
          <div role="alert" style={ALERT_STYLE}>
            No collection items could be loaded &#x2014; the auth-gated builder items
            endpoint is unavailable. Showing the collection without its items;
            you can still draft a brand-new wearable below.
          </div>
        ) : null}

        <WearableItemEditorWizard
          draft={d.draft}
          collections={collections}
          categories={d.categories}
          rarities={d.rarities}
          trackCtx={{
            sid: d.sid,
            story: STORY,
            variant: d.assignment.variant,
            experimentKey: d.assignment.experimentKey,
          }}
          initialStep={d.step ?? undefined}
        />
      </main>
    </CreatorHubChrome>
  );
}

const ALERT_STYLE: React.CSSProperties = {
  margin: "10px 24px 0",
  border: "1px solid color-mix(in srgb, var(--error) 35%, transparent)",
  background: "color-mix(in srgb, var(--error) 12%, transparent)",
  color: "color-mix(in srgb, var(--error) 45%, var(--text))",
  borderRadius: 8,
  padding: "10px 14px",
  fontSize: 13,
};

const CONNECT_NOTICE_STYLE: React.CSSProperties = {
  margin: "10px 24px 0",
  border: "1px solid color-mix(in srgb, var(--brand) 30%, transparent)",
  background: "color-mix(in srgb, var(--brand) 10%, transparent)",
  color: "var(--text)",
  borderRadius: 8,
  padding: "10px 14px",
  fontSize: 13,
};
