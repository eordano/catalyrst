import { foundationFetch } from "@data/lib/catalyst/builder/foundation-fetch";
import { listLinkedProviders, type LinkedProvider } from "@data/lib/catalyst/builder/linked-providers";
import { parseCollectionType } from "@features/stories/creator-hub/wearable-create-collection/machine";
import { useEffect, useMemo, useRef, useState } from "react";
import { collectionDraftWriter } from "@data/lib/catalyst/builder/collection-drafts";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useChromeAuth } from "@ui/web/frames/chrome-auth";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import CreateCollectionWizard, {
  type WizardOptions,
} from "@features/stories/creator-hub/wearable-create-collection/CreateCollectionWizard";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.wearables.collections.new";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("New Collection");

const STORY: StoryId = "creator-hub/wearable-create-collection";

function buildOptions(): WizardOptions {
  return {
    feePerItem: 100,
    nameSuggestions: [],
  };
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "cwc_create_collection_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const type = url.searchParams.get("type")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = { sid, step, type, assignment, options: buildOptions() };
  return wrap(payload);
}

export default function CreateWearableCollection({ loaderData }: Route.ComponentProps) {
  const { sid, step, type, assignment, options } = loaderData;
  const auth = useAuth();
  const { isConnected, address } = auth;
  const authRef = useRef(auth);
  authRef.current = auth;
  const linked = parseCollectionType(type) === "linked";
  const [providers, setProviders] = useState<{ owner?: string; rows: LinkedProvider[]; loading: boolean; error?: string }>({ rows: [], loading: false });
  const [providerId, setProviderId] = useState("");
  const [reload, setReload] = useState(0);
  const fetchFoundation = useMemo(() => foundationFetch(() => authRef.current), []);
  useEffect(() => {
    setProviderId("");
    if (!linked || !isConnected || !address) { setProviders({ rows: [], loading: false }); return; }
    const controller = new AbortController();
    setProviders({ owner: address, rows: [], loading: true });
    void listLinkedProviders(address, { fetch: fetchFoundation, signal: controller.signal }).then(rows => {
      if (!controller.signal.aborted) setProviders({ owner: address, rows, loading: false });
    }).catch(error => {
      if (!controller.signal.aborted) setProviders({ owner: address, rows: [], loading: false, error: error instanceof Error ? error.message : "Could not load your providers." });
    });
    return () => controller.abort();
  }, [linked, isConnected, address, reload, fetchFoundation]);
  const selectedProvider = providers.owner === address && providers.rows.some(row => row.id === providerId) ? providerId : "";
  const save = useMemo(() => collectionDraftWriter(() => {
    if (!authRef.current.isConnected) throw new Error("Sign in to save this collection.");
    return { fetch: fetchFoundation, address: authRef.current.address ?? undefined };
  }), [address, selectedProvider, fetchFoundation]);
  const { name } = useChromeAuth();

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

      <main className="cwc-create-collection-route">
        <CreateCollectionWizard
          key={`${address ?? "anonymous"}:${selectedProvider}`}
          mint={args => save({ ...args, thirdPartyId: selectedProvider })}
          provider={linked ? { value: selectedProvider, options: providers.rows, loading: providers.loading,
            error: !isConnected ? "Sign in with a provider manager wallet." : providers.error,
            onChange: setProviderId, onRetry: () => !isConnected ? openSignIn() : setReload(value => value + 1) } : undefined}
          trackCtx={{
            sid,
            story: STORY,
            variant: assignment.variant,
            experimentKey: assignment.experimentKey,
          }}
          options={options}
          initialStep={step ?? undefined}
          initialType={type ?? undefined}
        />
      </main>
    </CreatorHubChrome>
  );
}
