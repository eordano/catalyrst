import { useEffect, useRef, useState } from "react";
import { useNavigate, useRevalidator, useSearchParams } from "react-router";

import WearablesHomeView from "@ui/creatorhub/pages/WearablesHomeView";

import { useAuth } from "@data/lib/auth/index";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import {
  readView,
  toCollectionCard,
  type CollectionCardVM,
} from "@data/lib/catalyst/builder/collections";
import { listCollectionDrafts, type CollectionDraftSummary } from "@data/lib/catalyst/builder/collection-drafts";
import { listItemDrafts, type ItemDraftSummary } from "@data/lib/catalyst/builder/drafts";
import { loadCollections } from "@data/lib/catalyst/builder/collections.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoaderWith } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.wearables._index";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Collections");

const STORY: StoryId = "creator-hub/wearables-home";

const FALLBACK: Assignment = {
  variant: "wearables-home",
  flags: { showWearablesHome: true },
  experimentKey: "creator_wearables_home",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const view = readView(url.searchParams.get("view"));
  const address =
    url.searchParams.get("address")?.trim().toLowerCase() ||
    readWallet(request) ||
    "";

  const { sid, wrap, data } = await storyLoaderWith(
    request,
    STORY,
    FALLBACK,
    async () => {
      let collections: CollectionCardVM[] = [];
      let error = false;
      if (address) {
        const res = await loadCollections(address, request.signal);
        error = res.error;
        collections = res.collections.map(toCollectionCard);
      }
      return { collections, error };
    },
  );
  const { collections, error } = data;

  const payload = { sid, view, address, error, collections };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  view: "grid" | "list";
  address: string;
  error: boolean;
  collections: CollectionCardVM[];
};

export default function CreatorWearablesHome({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData as unknown as LoaderData;
  return <WearablesHome {...d} />;
}

function WearablesHome({
  sid,
  view,
  error,
  address: loaderAddress,
  collections,
}: LoaderData) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const revalidator = useRevalidator();
  const { isConnected, address, fetch: authFetch } = useAuth();
  const [draftState, setDraftState] = useState<{ address: string; rows: ItemDraftSummary[]; collections: CollectionDraftSummary[]; error: boolean } | null>(null);
  const [draftRetry, setDraftRetry] = useState(0);
  useEffect(() => {
    if (!isConnected || !address) { setDraftState(null); return; }
    const controller = new AbortController();
    const opts = { fetch: authFetch, signal: controller.signal };
    Promise.all([listItemDrafts(opts), listCollectionDrafts(opts)])
      .then(([rows, collections]) => { if (!controller.signal.aborted) setDraftState({ address, rows, collections, error: false }); })
      .catch(() => { if (!controller.signal.aborted) setDraftState({ address, rows: [], collections: [], error: true }); });
    return () => controller.abort();
  }, [isConnected, address, authFetch, draftRetry]);
  const ownDrafts = isConnected && draftState?.address === address ? draftState : null;
  const name = useProfileName(address, isConnected);

  useEffect(() => {
    if (isConnected && address && loaderAddress === "") {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("address", address);
          return next;
        },
        { replace: true, preventScrollReset: true },
      );
    }
  }, [isConnected, address, loaderAddress, setSearchParams]);

  const rescoping = isConnected && Boolean(address) && loaderAddress === "";

  useHomeView(sid, collections.length);

  function updateParam(key: string, value: string) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (value) next.set(key, value);
        else next.delete(key);
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function selectView(next: "grid" | "list") {
    if (next === view) return;
    track("creator_wearables_view_changed", { view: next }, { sid, story: STORY });
    updateParam("view", next === "grid" ? "" : next);
  }

  function onCardClick(id: string, kind: string) {
    track("creator_wearables_card_clicked", { id, kind }, { sid, story: STORY });
    navigate(`/create/wearables/collections/${encodeURIComponent(id)}`);
  }

  function onSignIn() {
    track("creator_wearables_signin_clicked", {}, { sid, story: STORY });
    openSignIn();
  }

  function onRetry() {
    if (typeof revalidator.revalidate === "function") {
      revalidator.revalidate();
    } else {
      navigate(0);
    }
  }
  const retrying = revalidator.state === "loading";

  return (
    <WearablesHomeView
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      error={error}
      retrying={retrying}
      rescoping={rescoping}
      collections={[
        ...(ownDrafts?.collections ?? []).map(collection => ({
          id: collection.id, name: collection.name, type: collection.third_party_id ? "third_party" as const : "collection" as const,
          status: collection.is_approved ? "synced" as const : collection.is_published ? "under_review" as const : "unsynced" as const,
          count: collection.item_count ?? ownDrafts!.rows.filter(item => item.collection_id === collection.id).length, thumbs: [],
        })),
        ...collections.filter(collection => !ownDrafts?.collections.some(draft => draft.id === collection.id)),
      ]}
      drafts={ownDrafts?.rows.filter(item => item.type === "wearable" && !item.collection_id) ?? []}
      draftsError={ownDrafts?.error ?? false}
      onRetryDrafts={() => setDraftRetry(value => value + 1)}
      onOpenDraft={id => navigate(`/create/wearables/item-editor?item=${encodeURIComponent(id)}&step=model`)}
      view={view}
      onSignIn={onSignIn}
      onSelectView={selectView}
      onOpen={onCardClick}
      onRetry={onRetry}
    />
  );
}

function useHomeView(sid: string, count: number) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track("creator_wearables_home_viewed", { count }, { sid, story: STORY });
  }, [sid, count]);
}
