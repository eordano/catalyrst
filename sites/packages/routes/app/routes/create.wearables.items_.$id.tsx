import { useEffect, useRef } from "react";
import { Link, useNavigate } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import ChItemDetail from "@ui/creatorhub/pages/ChItemDetail";
import ChNotFound from "@ui/creatorhub/pages/ChNotFound";
import "@ui/creatorhub/pages/chitemdetail.css";
import "@ui/creatorhub/pages/chnotfound.css";

import { type BuilderItem } from "@data/lib/catalyst/builder/items";
import { loadCreatorItem } from "@data/lib/catalyst/creator-hub/wearable-item-detail.server";
import { useAuth } from "@data/lib/auth/context";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.wearables.items_.$id";
import type { StoryId } from "@core/lib/telemetry/story-id";

export function meta({ loaderData }: Route.MetaArgs) {
  const item = (loaderData as { item?: { name?: string } | null } | undefined)
    ?.item;
  return creatorHubMeta(item?.name?.trim() || "Item");
}

const STORY: StoryId = "creator-hub/wearable-item-detail";

const FALLBACK: Assignment = {
  variant: "read-only-detail",
  flags: { liveItem: true },
  experimentKey: "creator-wearable-item-detail",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const { item, fallback } = await loadCreatorItem(id, { signal: request.signal });

  const payload = { sid, id, item, fallback };

  return wrap(payload);
}

export default function CreatorWearableItemDetailRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  return (
    <ItemDetailView sid={d.sid} id={d.id} item={d.item} fallback={d.fallback} />
  );
}

function ItemDetailView({
  sid,
  id,
  item,
  fallback,
}: {
  sid: string;
  id: string;
  item: BuilderItem | null;
  fallback: boolean;
}) {
  const navigate = useNavigate();
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);

  const last = useRef<string | null>(null);
  useEffect(() => {
    if (last.current === id) return;
    last.current = id;
    track(
      "creator_item_detail_viewed",
      { id, type: item?.type ?? "missing", fallback },
      { sid, story: STORY },
    );
  }, [sid, id, item?.type, fallback]);

  if (!item) {
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
          subtitle="This item doesn't exist or was removed"
          backLabel="Back to Collections"
          onBack={() => navigate("/create/wearables")}
        />
      </CreatorHubChrome>
    );
  }

  const resolvedItem = item;

  function onClick(e: React.MouseEvent<HTMLDivElement>) {
    const el = e.target as HTMLElement;
    const btn = el.closest<HTMLElement>("button.bditemdetail__editbtn");
    if (!btn) return;

    const head = btn.closest<HTMLElement>(".bditemdetail__cardhead");
    const titleEl = head?.querySelector<HTMLElement>(".bditemdetail__title");
    const title = titleEl?.textContent?.trim() ?? "";

    let dest: string | null = null;
    if (title === resolvedItem.name) {
      dest = `/create/wearables/item-editor?item=${encodeURIComponent(resolvedItem.id)}`;
    } else if (title === "Representations") {
      dest = `/create/wearables/item-editor?item=${encodeURIComponent(resolvedItem.id)}&step=model`;
    }
    if (!dest) return;

    track(
      "creator_item_edit_clicked",
      { id: resolvedItem.id },
      { sid, story: STORY },
    );
    navigate(dest);
  }

  return (
    <CreatorHubChrome
      active="collections"
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => openSignIn()}
    >
      <div className="creator-wearable-item-detail-route" onClick={onClick}>
        <nav
          className="bditemdetail__breadcrumb"
          aria-label="Breadcrumb"
          style={{ padding: "12px 24px 0" }}
        >
          <Link
            to={href("/create/wearables")}
            prefetch="intent"
            style={{
              color: "var(--bdid-subtitle, #736e7d)",
              textDecoration: "none",
            }}
          >
            &#x2190; Collections
          </Link>
        </nav>
        <ChItemDetail
          bare
          item={resolvedItem as React.ComponentProps<typeof ChItemDetail>["item"]}
          onBack={() => navigate("/create/wearables")}
        />
      </div>
    </CreatorHubChrome>
  );
}
