import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import { useAuth, walletProvider } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useChromeAuth } from "@ui/web/frames/chrome-auth";
import {
  fetchCollectionItems,
  fetchCollectionMeta,
} from "@data/lib/catalyst/builder/collection-detail";
import {
  buildPublishCollection,
  buildSummary,
  toPublishItems,
} from "@data/lib/catalyst/creator-hub/wearable-publish-collection";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import type { SummaryView } from "@features/stories/creator-hub/wearable-publish-collection/PublishCollectionWizard";
import LiveLinkedPublishWizard from "@features/stories/creator-hub/wearable-publish-collection/LiveLinkedPublishWizard";
import { linkedPublicationFlow } from "@data/lib/catalyst/builder/linked-publication-flow";
import LivePublishCollectionWizard from "@features/stories/creator-hub/wearable-publish-collection/LivePublishCollectionWizard";
import { collectionPublicationFlow } from "@data/lib/catalyst/builder/collection-publication-flow";
import type { PublishCollection } from "@features/stories/creator-hub/wearable-publish-collection/machine";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.wearables.publish";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Publish Collection");

const STORY: StoryId = "creator-hub/wearable-publish-collection";
const EXPERIMENT_KEY = "bd_wearable_publish_wizard";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const collectionId = url.searchParams.get("collection")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = { sid, collectionId: collectionId ?? "", assignment };
  return wrap(payload);
}

export default function CreateWearablesPublishRoute({
  loaderData,
}: Route.ComponentProps) {
  const { sid, collectionId, assignment } =
    loaderData;
  const auth = useAuth();
  const { isConnected, address } = auth;
  const authRef = useRef(auth);
  authRef.current = auth;
  const flow = useMemo(() => collectionPublicationFlow(() => {
    const current = authRef.current;
    if (!current.address || !current.isConnected) throw new Error("Sign in with the collection owner to publish.");
    return {address:current.address, provider:walletProvider(), fetch:current.fetch, sign:current.sign};
  }), [address]);
  const linkedFlow = useMemo(() => linkedPublicationFlow(() => {
    const current = authRef.current;
    if (!current.address || !current.isConnected) throw new Error("Sign in with the collection owner to publish.");
    return {address:current.address, provider:walletProvider(), fetch:current.fetch, sign:current.sign};
  }), [address]);
  const { name } = useChromeAuth();

  const live = useLivePublish(collectionId);
  const hasLive = live.phase === "ready" || live.phase === "empty";
  const effFallback = !hasLive;
  const loading = live.phase === "loading";

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

      <main className="cwpc-route">
        {effFallback ? (
          <p className="cwpc-route__sim" role={loading ? "status" : "alert"}>
            {!collectionId ? (
              <>
                No collection is selected yet. Pick the collection you want to
                publish from your collections list.{" "}
                <Link className="cwpc-route__sim-cta" to={href("/create/wearables")}>
                  Choose a collection
                </Link>
              </>
            ) : !isConnected ? (
              <>
                Sign in to load this collection&apos;s items.{" "}
                <button
                  type="button"
                  className="cwpc-route__sim-cta"
                  onClick={() => {
                    openSignIn();
                  }}
                >
                  Sign in
                </button>
              </>
            ) : loading || live.phase === "idle" ? (
              <>Loading this collection&apos;s items&#x2026;</>
            ) : (
              <>
                We couldn&apos;t load this collection&apos;s items. Check your
                connection and try again.{" "}
                <button
                  type="button"
                  className="cwpc-route__sim-cta"
                  onClick={live.retry}
                >
                  Try again
                </button>
              </>
            )}
          </p>
        ) : null}

        {hasLive && isConnected && live.data!.linked && <LiveLinkedPublishWizard
          key={`${address}:${collectionId}:linked`}
          collection={live.data!.collection} summary={live.data!.summary} flow={linkedFlow}
          trackCtx={{sid,story:STORY,variant:assignment.variant,experimentKey:assignment.experimentKey}}
        />}
        {hasLive && isConnected && !live.data!.linked && <LivePublishCollectionWizard
          key={`${address}:${collectionId}`}
          collection={live.data!.collection}
          summary={live.data!.summary}
          flow={flow}
          trackCtx={{
            sid,
            story: STORY,
            variant: assignment.variant,
            experimentKey: assignment.experimentKey,
          }}
        />}
      </main>
    </CreatorHubChrome>
  );
}

type LivePublish = { collection: PublishCollection; summary: SummaryView; linked: boolean };

type LivePublishPhase = "idle" | "loading" | "ready" | "empty" | "error";

type LivePublishState = {
  data: LivePublish | null;
  phase: LivePublishPhase;
  retry: () => void;
};

function useLivePublish(id: string): LivePublishState {
  const auth = useAuth();
  const connected = auth.isConnected;
  const address = auth.address;
  const authFetch = auth.fetch;
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{
    data: LivePublish | null;
    phase: LivePublishPhase;
  }>({ data: null, phase: "idle" });

  useEffect(() => {
    if (!connected || !id) {
      setState({ data: null, phase: "idle" });
      return;
    }
    let cancelled = false;
    setState({ data: null, phase: "loading" });
    const fetchImpl: typeof fetch = (input, init) =>
      authFetch(typeof input === "string" ? input : input.toString(), init);
    const opts = { fetchImpl, base: "" };
    Promise.all([
      fetchCollectionItems(id, opts),
      fetchCollectionMeta(id, opts),
    ])
      .then(([{ wearables, emotes }, meta]) => {
        if (cancelled) return;
        const items = toPublishItems(wearables, emotes);
        setState({
          data: {
            collection: buildPublishCollection(id, items, meta?.name ?? ""),
            summary: buildSummary(id, wearables, emotes, meta),
            linked: meta?.urn?.startsWith("urn:decentraland:matic:collections-thirdparty:") ?? false,
          },
          phase: items.length === 0 ? "empty" : "ready",
        });
      })
      .catch(() => {
        if (!cancelled) setState({ data: null, phase: "error" });
      });
    return () => {
      cancelled = true;
    };
  }, [connected, address, authFetch, id, attempt]);

  return {
    data: state.data,
    phase: state.phase,
    retry: () => setAttempt((a) => a + 1),
  };
}
