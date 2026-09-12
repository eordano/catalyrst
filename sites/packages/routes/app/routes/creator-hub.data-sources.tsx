import { useRevalidator, useSearchParams } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import ChDataSourcesPage, {
  SOURCE_FILTERS,
  type SourceFilter,
} from "@ui/creatorhub/pages/ChDataSourcesPage";

import { SOURCE_REGISTRY } from "@data/lib/catalyst/creator-hub/data-sources";
import { probeSources } from "@data/lib/catalyst/creator-hub/data-sources.server";
import {
  buildLedgerGroups,
  type LedgerResults,
} from "@features/components/creator-hub/source-ledger";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.data-sources";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Data sources");

const STORY: StoryId = "creator-hub/data-sources";

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "ch_data_sources",
};

function parseFilter(raw: string | null): SourceFilter {
  const hit = SOURCE_FILTERS.find((f) => f.id === raw);
  return hit ? hit.id : "all";
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const filter = parseFilter(url.searchParams.get("filter"));

  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK);

  const readAt = new Date().toISOString();

  const probed = await probeSources({ signal: request.signal });

  const results: LedgerResults = {};
  for (const entry of probed) {
    if (entry.result) results[entry.id] = entry.result;
  }

  const groups = buildLedgerGroups(SOURCE_REGISTRY, results);

  return wrap({ sid, filter, readAt, groups });
}

export default function CreatorHubDataSourcesRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const [searchParams, setSearchParams] = useSearchParams();
  const revalidator = useRevalidator();

  const filter = parseFilter(searchParams.get("filter")) || d.filter;

  function onFilterChange(next: SourceFilter) {
    const sp = new URLSearchParams(searchParams);
    if (next === "all") sp.delete("filter");
    else sp.set("filter", next);
    setSearchParams(sp, { preventScrollReset: true, replace: true });
  }

  return (
    <CreatorHubChrome active="sources">
      <ChDataSourcesPage
        groups={d.groups}
        readAt={d.readAt}
        filter={filter}
        onFilterChange={onFilterChange}
        onRefresh={() => revalidator.revalidate()}
        refreshing={revalidator.state === "loading"}
      />
    </CreatorHubChrome>
  );
}
