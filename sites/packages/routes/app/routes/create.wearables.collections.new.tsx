import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
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
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);

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
