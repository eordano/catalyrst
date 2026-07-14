import { useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router";

import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import NewShopTabs from "@ui/marketplace/new-shop/NewShopTabs";
import {
  MkListsOverview,
  MkListsOpenList,
  MkListsSignedOut,
} from "@ui/marketplace/pages/MkListsPage";
import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/marketplace/new-shop/newshoptabs.css";

import { openSignIn } from "@features/components/auth/signin-store";
import { useAuth } from "@data/lib/auth/context";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { tryQuoteCreditItems } from "@data/lib/catalyst/marketplace/credit-quotes";
import { parseItemId } from "@data/lib/catalyst/marketplace/index";
import {
  loadLists,
  loadList,
  toListCard,
  toListItemCard,
  type List,
  type ListCard,
  type ListItemCard,
} from "@data/lib/catalyst/marketplace/lists";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.lists";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/lists";

const SHOP_TABS = [
  { id: "overview", label: "Overview", href: "/shop" },
  { id: "all-assets", label: "All Assets", href: "/shop?tab=all-assets" },
  { id: "names", label: "NAMEs", href: "/marketplace/names" },
  { id: "my-assets", label: "My Assets", href: "/marketplace/account" },
  { id: "my-favorites", label: "My Favorites", href: "/shop?tab=my-favorites" },
  { id: "cart", label: "Cart", href: "/marketplace/cart" },
] as const;

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "marketplace_lists",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const listId = url.searchParams.get("listId")?.trim() ?? "";
  const address =
    url.searchParams.get("address")?.trim().toLowerCase() || readWallet(request) || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const loaded = address ? await loadLists(address, { signal: request.signal }) : [];
  const overview: ListCard[] = (loaded ?? []).map(toListCard);
  const overviewUnavailable = loaded === null;
  const open: List | null =
    address && listId
      ? await loadList(listId, address, { signal: request.signal })
      : null;

  const quoted = open
    ? await tryQuoteCreditItems(
        open.items.map((it) => {
          const ref = parseItemId(it.id);
          return ref
            ? { itemId: ref.itemId, collection: ref.contractAddress }
            : null;
        }),
        { signal: request.signal },
      )
    : null;

  const payload = {
    sid,
    listId,
    address,
    overview,
    overviewUnavailable,
    open: open
      ? {
          id: open.id,
          name: open.name,
          description: open.description,
          isPrivate: open.isPrivate,
          pickedByUser: open.pickedByUser,
          itemsCount: open.itemsCount,
          items: open.items.map((it, i) =>
            toListItemCard(it, quoted?.[i] ?? null),
          ),
        }
      : null,
  };

  return wrap(payload);
}

type OpenList = {
  id: string;
  name: string;
  description: string | null;
  isPrivate: boolean;
  pickedByUser: boolean | null;
  itemsCount: number;
  items: ListItemCard[];
};

function isModifiedEvent(e: React.MouseEvent) {
  return e.metaKey || e.altKey || e.ctrlKey || e.shiftKey;
}

export default function MarketplaceLists({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const [, setSearchParams] = useSearchParams();
  const auth = useAuth();

  useEffect(() => {
    if (d.address) return;
    const me = auth.address;
    if (!me) return;
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("address", me);
        return next;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [d.address, auth.address, setSearchParams]);

  return (
    <ChromeShell
      className="mk"
      ariaLabel="My Lists"
      topbar={<DclTopBar variant="sites" active="shop" />}
      subnav={false}
    >
      <NewShopTabs tabs={SHOP_TABS} active="my-favorites" />
      {!d.address ? (
        <MkListsSignedOut connecting={!!auth.address} onSignIn={() => openSignIn()} />
      ) : d.open ? (
        <OpenListView sid={d.sid} list={d.open} />
      ) : (
        <ListsOverview
          sid={d.sid}
          lists={d.overview}
          unavailable={d.overviewUnavailable}
        />
      )}
    </ChromeShell>
  );
}

function ListsOverview({
  sid,
  lists,
  unavailable,
}: {
  sid: string;
  lists: ListCard[];
  unavailable: boolean;
}) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  useListsViewed(sid, lists.length);

  function openList(id: string) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("listId", id);
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function onOpenList(id: string, e: React.MouseEvent<HTMLAnchorElement>) {
    if (e.metaKey || e.ctrlKey || e.button !== 0) return;
    e.preventDefault();
    openList(id);
  }

  function onFavoritesClick(e: React.MouseEvent<HTMLAnchorElement>) {
    if (e.button !== 0 || isModifiedEvent(e)) return;
    e.preventDefault();
    navigate("/shop?tab=my-favorites");
  }

  if (unavailable) {
    return (
      <div className="mklists">
        <div className="mklists__head">
          <h1 className="mklists__title">My Lists</h1>
        </div>
        <p className="mklists__sub" role="alert">
          Your lists could not be read just now. This is not a claim that you
          have none {"\u{2014}"} try again in a moment.
        </p>
      </div>
    );
  }

  return (
    <MkListsOverview
      lists={lists.map((l) => ({
        ...l,
        pickedByUser: l.pickedByUser ?? undefined,
      }))}
      onOpenList={onOpenList}
      onFavoritesClick={onFavoritesClick}
    />
  );
}

function OpenListView({ sid, list }: { sid: string; list: OpenList }) {
  const navigate = useNavigate();

  useListOpened(sid, list.id, list.items.length);

  function onItemClick(itemId: string, e: React.MouseEvent<HTMLAnchorElement>) {
    track(
      "mk_list_item_clicked",
      { list_id: list.id, item_id: itemId },
      { sid, story: STORY },
    );
    if (e.button !== 0 || isModifiedEvent(e)) return;
    e.preventDefault();
    navigate(`/marketplace/${encodeURIComponent(itemId)}`);
  }

  function onBackClick(e: React.MouseEvent<HTMLAnchorElement>) {
    if (e.button !== 0 || isModifiedEvent(e)) return;
    e.preventDefault();
    navigate("/marketplace/lists");
  }

  return (
    <MkListsOpenList
      list={{ ...list, pickedByUser: list.pickedByUser ?? undefined }}
      onItemClick={onItemClick}
      onBackClick={onBackClick}
    />
  );
}

function useListsViewed(sid: string, count: number) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track("mk_lists_viewed", { count }, { sid, story: STORY });
  }, [sid, count]);
}

function useListOpened(sid: string, listId: string, itemsCount: number) {
  const lastId = useRef<string | null>(null);
  useEffect(() => {
    if (lastId.current === listId) return;
    lastId.current = listId;
    track(
      "mk_list_opened",
      { list_id: listId, items_count: itemsCount },
      { sid, story: STORY },
    );
  }, [sid, listId, itemsCount]);
}
