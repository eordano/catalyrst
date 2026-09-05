import { useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router";

import MkCollectionView from "@ui/marketplace/pages/MkCollectionView";

import { tryQuoteCreditItems } from "@data/lib/catalyst/marketplace/credit-quotes";
import {
  fetchCollection,
  fetchCollectionItems,
  parseItemId,
  toCollectionHeader,
  toCollectionItemRow,
  toCollectionStats,
  type CatalogItem,
  type Collection,
  type CollectionHeader,
  type CollectionItemRow,
  type CollectionStats,
} from "@data/lib/catalyst/marketplace/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import { trackCreatorFunnel } from "@core/lib/telemetry/creator-funnel";

import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";

export const handle = { agentMarkdown: "collectionDetail" } satisfies AgentMarkdownHandle;

import type { Route } from "./+types/marketplace.collection";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/collection";
const ITEMS_LIMIT = 60;

const RARITY_VALUES = [
  "unique",
  "mythic",
  "exotic",
  "legendary",
  "epic",
  "rare",
  "uncommon",
  "common",
] as const;
type MkRarity = (typeof RARITY_VALUES)[number];

function toMkRarity(rarity: string): MkRarity {
  return (RARITY_VALUES as readonly string[]).includes(rarity)
    ? (rarity as MkRarity)
    : "common";
}

function toMkItem(row: CollectionItemRow) {
  return {
    id: row.id,
    name: row.name,
    category: row.category === "emote" ? ("emote" as const) : ("wearable" as const),
    sub: row.sub,
    rarity: toMkRarity(row.rarity),
    available: row.available,
    price: row.price,
    credits: row.credits,
    image: row.image ?? undefined,
  };
}

const VALID_SORTS = new Set([
  "recently_listed",
  "cheapest",
  "most_expensive",
  "recently_sold",
]);

function readSort(params: URLSearchParams): string {
  const raw = params.get("sortBy")?.trim() ?? "";
  return VALID_SORTS.has(raw) ? raw : "recently_listed";
}

function readContract(params: URLSearchParams): string {
  return params.get("contract")?.trim() ?? "";
}

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "marketplace_collection",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const contract = readContract(url.searchParams);
  const sortBy = readSort(url.searchParams);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let collection: Collection | null = null;
  let items: CatalogItem[] = [];
  let fallback = false;

  if (contract) {
    const [liveCollection, liveItems] = await Promise.all([
      fetchCollection(contract, { signal: request.signal }).catch(() => {
        fallback = true;
        return null;
      }),
      fetchCollectionItems(
        contract,
        { first: ITEMS_LIMIT, sortBy },
        { signal: request.signal },
      )
        .then((r) => r.data)
        .catch(() => {
          fallback = true;
          return [] as CatalogItem[];
        }),
    ]);
    collection = liveCollection;
    items = liveItems;
  }

  const header: CollectionHeader | null = collection
    ? toCollectionHeader(collection)
    : null;

  const quotedCredits = await tryQuoteCreditItems(
    items.map((it) => {
      const ref = parseItemId(it.id);
      return ref ? { itemId: ref.itemId, collection: ref.contractAddress } : null;
    }),
    { signal: request.signal },
  );
  const creditsById = new Map<string, string | null>();
  items.forEach((it, i) => creditsById.set(it.id, quotedCredits[i] ?? null));

  const rows: CollectionItemRow[] = items.map((it, i) =>
    toCollectionItemRow(it, quotedCredits[i] ?? null),
  );
  const stats: CollectionStats = toCollectionStats(
    collection,
    items,
    (it) => creditsById.get(it.id) ?? null,
  );

  const payload = {
    sid,
    contract,
    sortBy,
    header,
    items: rows,
    stats,
    fallback,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  contract: string;
  sortBy: string;
  header: CollectionHeader | null;
  items: CollectionItemRow[];
  stats: CollectionStats;
  fallback: boolean;
};

export default function MarketplaceCollection({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  useCollectionViewed(d);

  const navigate = useNavigate();
  const [, setSearchParams] = useSearchParams();
  const rootRef = useRef<HTMLDivElement | null>(null);

  const nameToId = new Map<string, string>();
  for (const it of d.items) {
    if (!nameToId.has(it.name)) nameToId.set(it.name, it.id);
  }

  function onSort(value: string) {
    track(
      "mk_collection_sorted",
      { sort_by: value, contract: d.contract },
      { sid: d.sid, story: STORY },
    );
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (value && value !== "recently_listed") next.set("sortBy", value);
        else next.delete("sortBy");
        return next;
      },
      { preventScrollReset: true },
    );
  }

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    function handler(e: MouseEvent) {
      const target = e.target as HTMLElement | null;
      const cell = target?.closest?.("a.cp__assetcell");
      if (!cell) return;
      e.preventDefault();
      const name = cell.querySelector(".cp__assetname")?.textContent?.trim() ?? "";
      const id = nameToId.get(name);
      if (!id) return;
      track(
        "mk_collection_item_clicked",
        { item_id: id, contract: d.contract },
        { sid: d.sid, story: STORY },
      );
      navigate(`/marketplace/${encodeURIComponent(id)}`);
    }
    root.addEventListener("click", handler);
    return () => root.removeEventListener("click", handler);
  }, [d.items, d.sid, d.contract, navigate]);

  return (
    <MkCollectionView
      rootRef={rootRef}
      stats={d.stats}
      sortBy={d.sortBy}
      collection={d.header ?? undefined}
      items={d.items.map(toMkItem)}
      onSort={onSort}
    />
  );
}

function useCollectionViewed(d: LoaderData) {
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${d.contract}|${d.sortBy}`;
    if (lastKey.current === key) return;
    lastKey.current = key;

    const empty = !d.header || d.items.length === 0;
    if (empty) {
      track(
        "mk_collection_empty",
        { contract: d.contract, fallback: d.fallback },
        { sid: d.sid, story: STORY },
      );
      return;
    }
    track(
      "mk_collection_viewed",
      {
        contract: d.contract,
        item_count: d.stats.itemCount,
        floor: d.stats.floor,
        sort_by: d.sortBy,
      },
      { sid: d.sid, story: STORY },
    );
    trackCreatorFunnel(
      "creator_collection_viewed",
      { contract: d.contract, item_count: d.stats.itemCount },
      { sid: d.sid },
    );
    trackCreatorFunnel(
      "creator_store_viewed",
      { contract: d.contract, floor: d.stats.floor },
      { sid: d.sid },
    );
  }, [d]);
}
