import { useEffect, useRef } from "react";
import type { CSSProperties } from "react";
import { Link, useNavigate, useRevalidator, useSearchParams } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import Dropdown from "@ui/components/Dropdown";
import SearchField from "@ui/atoms/SearchField";
import Button from "@ui/atoms/Button";
import EmptyState from "@ui/components/EmptyState";
import "@ui/creatorhub/pages/chmanage.css";

import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import {
  applyFilter,
  applySearch,
  applySort,
  readFilter,
  readSort,
  toWorldCard,
  SORT_TO_LABEL,
  SORT_LABEL_TO_VALUE,
  type WorldCardVM,
  type WorldsFilter,
  type WorldsSort,
} from "@data/lib/catalyst/creator-hub/manage-worlds";
import { loadManageWorlds } from "@data/lib/catalyst/creator-hub/manage-worlds.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.manage";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Worlds");

const STORY: StoryId = "creator-hub/manage-worlds";

const FILTER_CHIPS: { label: string; value: WorldsFilter }[] = [
  { label: "Published", value: "published" },
  { label: "Not published", value: "unpublished" },
];
const SORT_OPTIONS = [SORT_TO_LABEL.last_published, SORT_TO_LABEL.domain];

const FALLBACK: Assignment = {
  variant: "manage-hub",
  flags: { showManageHub: true },
  experimentKey: "creator-hub-manage-worlds",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const filter = readFilter(url.searchParams.get("filter"));
  const sort = readSort(url.searchParams.get("sort"));
  const search = url.searchParams.get("search")?.trim() ?? "";
  const address = url.searchParams.get("address")?.trim() || readWallet(request) || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const d = await loadManageWorlds(address, request.signal).catch(() => null);

  const worlds = d?.worlds ?? [];
  const filtered = applyFilter(worlds, filter);
  const searched = filter === "published" ? applySearch(filtered, search) : filtered;
  const ordered = filter === "published" ? applySort(searched, sort) : searched;
  const cards = ordered.map(toWorldCard);

  const payload = {
    sid,
    address,
    filter,
    sort,
    search,
    cards,
    nameCount: d?.names.length ?? 0,
    fallback: d == null,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  address: string;
  filter: WorldsFilter;
  sort: WorldsSort;
  search: string;
  cards: WorldCardVM[];
  nameCount: number;
  fallback: boolean;
};

export default function CreatorHubManage({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return <ManageView {...d} />;
}

function ManageView({
  sid,
  address,
  filter,
  sort,
  search,
  cards,
  fallback,
}: LoaderData) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const revalidator = useRevalidator();
  const { isConnected, address: walletAddress } = useAuth();
  const name = useProfileName(walletAddress, isConnected);
  const isEmpty = cards.length === 0;
  const rescoping = isConnected && Boolean(walletAddress) && !address;

  const needsConnect = !address && !rescoping && !fallback;

  useEffect(() => {
    if (isConnected && walletAddress && !address) {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("address", walletAddress);
          return next;
        },
        { replace: true, preventScrollReset: true },
      );
    }
  }, [isConnected, walletAddress, address, setSearchParams]);

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${filter}|${sort}|${search}|${cards.length}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    track(
      "ch_manage_viewed",
      {
        count: cards.length,
        filter,
        sort,
        search: search || null,
        address: address || null,
      },
      { sid, story: STORY },
    );
    if (isEmpty) {
      track("ch_manage_empty_viewed", { filter }, { sid, story: STORY });
    }
  }, [sid, filter, sort, search, cards.length, isEmpty, address]);

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

  function onFilter(next: WorldsFilter) {
    if (next === filter) return;
    track("ch_manage_filter_changed", { filter: next }, { sid, story: STORY });
    updateParam("filter", next === "published" ? "" : next);
  }

  function onSort(label: string) {
    const next = SORT_LABEL_TO_VALUE[label] ?? "last_published";
    if (next === sort) return;
    track("ch_manage_sorted", { sort: next }, { sid, story: STORY });
    updateParam("sort", next === "last_published" ? "" : next);
  }

  function onSearch(q: string) {
    track("ch_manage_searched", { q: q || null }, { sid, story: STORY });
    updateParam("search", q);
  }

  function onCardClick(card: WorldCardVM) {
    track(
      "ch_manage_card_clicked",
      { id: card.id, role: card.role },
      { sid, story: STORY },
    );
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
    <CreatorHubChrome
      active="manage"
      signedIn={isConnected}
      account={walletAddress ?? ""}
      name={name}
      onSignIn={() => {
        openSignIn();
      }}
    >
      <section className="chm">
        <div className="chm__container">
          <h1 className="chm__title">Manage Worlds</h1>

          {address ? (
            <Link
              to={`/creator-hub/worlds-storage?quota=1&address=${encodeURIComponent(address)}`}
              prefetch="intent"
              className="chm__storagelink"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                margin: "0 0 14px",
                fontSize: 13,
                fontWeight: 600,
                color: "var(--ink-85)",
                textDecoration: "none",
                border: "1px solid var(--line)",
                borderRadius: "var(--r-control)",
                padding: "6px 12px",
              }}
              onClick={() =>
                track("ch_manage_storage_opened", {}, { sid, story: STORY })
              }
            >
              Your Storage
            </Link>
          ) : null}

          <div className="chm__row">
            <div className="chm__content">
              {rescoping ? (
                <div
                  className="chm__loader"
                  style={{ flexDirection: "column", gap: 14 }}
                  role="status"
                  aria-live="polite"
                >
                  <span className="chm__spinner" aria-hidden="true" />
                  <p style={{ margin: 0, color: "var(--ink-45)", fontSize: 14 }}>
                    Loading your worlds&#x2026;
                  </p>
                </div>
              ) : needsConnect ? (
                <EmptyState
                  className="es--card"
                  icon={ManageGlyph}
                  iconWash
                  title="Sign in to manage your Worlds"
                  subtitle="Sign in to view your owned NAMEs and published Worlds. Every NAME you own comes with a free World (a new NAME costs 100 MANA), and world storage grows at 100 Mb per 2,000 MANA, per LAND, and per NAME you hold."
                  actions={
                    <Button
                      variant="secondary"
                      onClick={() => openSignIn()}
                    >
                      Sign in
                    </Button>
                  }
                  variant={undefined}
                  tone={undefined}
                  actionsGap={undefined}
                  style={undefined}
                />
              ) : (
                <>
                  {fallback ? (
                    <div className="chm__sim" role="alert" style={MANAGE_ERROR}>
                      <span>
                        <strong>Couldn&apos;t load Worlds.</strong> The Worlds
                        service was unreachable just now, so this list is empty
                        rather than showing sample data.
                      </span>
                      <button
                        type="button"
                        onClick={onRetry}
                        disabled={retrying}
                        style={MANAGE_RETRY}
                      >
                        {retrying ? "Retrying\u{2026}" : "Try again"}
                      </button>
                    </div>
                  ) : null}
                  {address ? (
                    <>
                      <div className="chm__filtersbar">
                        <div className="chm__filtersleft">
                          <h2 className="chm__count">
                            {cards.length}{" "}
                            {cards.length === 1 ? "item" : "items"}
                          </h2>
                          <div className="chm__chipsfilter">
                            <span className="chm__filterlabel">Filter by</span>
                            {FILTER_CHIPS.map((f) => (
                              <button
                                key={f.value}
                                type="button"
                                className={
                                  "chm__filterchip" +
                                  (f.value === filter ? " is-active" : "")
                                }
                                aria-pressed={f.value === filter}
                                onClick={() => onFilter(f.value)}
                              >
                                {f.label}
                              </button>
                            ))}
                          </div>
                        </div>
                        {filter === "published" ? (
                          <div className="chm__filtersright">
                            <span className="chm__filterlabel">Sort by</span>
                            <Dropdown
                              ariaLabel="Sort by"
                              options={SORT_OPTIONS as never[]}
                              value={SORT_TO_LABEL[sort]}
                              defaultValue={undefined}
                              onChange={onSort}
                            />
                            <SearchField
                              placeholder="Search"
                              value={search}
                              onChange={onSearch}
                            />
                          </div>
                        ) : null}
                      </div>

                      {isEmpty ? (
                        <ManageEmpty filter={filter} />
                      ) : (
                        <div className="chm__list">
                          {cards.map((card) => (
                            <PublishedProjectCard
                              key={card.id}
                              card={card}
                              address={address}
                              onCardClick={onCardClick}
                            />
                          ))}
                        </div>
                      )}
                    </>
                  ) : null}
                </>
              )}
            </div>
          </div>
        </div>
      </section>
    </CreatorHubChrome>
  );
}

const MANAGE_ERROR: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 16,
  flexWrap: "wrap",
};
const MANAGE_RETRY: React.CSSProperties = {
  flex: "0 0 auto",
  border: "1px solid color-mix(in srgb, var(--error) 45%, transparent)",
  background: "color-mix(in srgb, var(--error) 16%, transparent)",
  color: "inherit",
  borderRadius: "var(--r-control)",
  padding: "6px 14px",
  fontSize: 13,
  fontWeight: 600,
  cursor: "pointer",
};

const ManageGlyph = (
  <svg viewBox="0 0 20 20" width="28" height="28" fill="none" aria-hidden="true">
    <path
      d="M3 5h14M3 10h14M3 15h9"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
    />
  </svg>
);

function ManageEmpty({ filter }: { filter: WorldsFilter }) {
  if (filter === "unpublished") {
    return (
      <EmptyState
        className="es--card"
        icon={ManageGlyph}
        iconWash
        title="No unpublished Worlds to display"
        subtitle="Mint a NAME to get a World of your own to publish Scenes to. Worlds whose owners added you as a collaborator also show up here."
        actions={
          <Link
            to={href("/creator-hub/claim-name")}
            prefetch="intent"
            className="btn btn--secondary btn--md"
            style={{ textDecoration: "none" }}
          >
            Mint a new NAME
          </Link>
        }
        variant={undefined}
        tone={undefined}
        actionsGap={undefined}
        style={undefined}
      />
    );
  }
  return (
    <EmptyState
      className="es--card"
      icon={ManageGlyph}
      iconWash
      title="No published Worlds to display"
      subtitle="Publish one of your Scenes to a World to see it here. Worlds whose owners added you as a collaborator also show up here."
      actions={
        <Link
          to={href("/create/scenes")}
          prefetch="intent"
          className="btn btn--secondary btn--md"
          style={{ textDecoration: "none" }}
        >
          View Scenes
        </Link>
      }
      variant={undefined}
      tone={undefined}
      actionsGap={undefined}
      style={undefined}
    />
  );
}

function PublishedProjectCard({
  card,
  address,
  onCardClick,
}: {
  card: WorldCardVM;
  address: string;
  onCardClick: (card: WorldCardVM) => void;
}) {
  const { displayName, role, deployment } = card;
  const roleLabel =
    role === "collaborator" ? "Collaborator" : role === "operator" ? "Operator" : null;
  const worldBase = `/creator-hub/world-settings?world=${encodeURIComponent(card.id)}`;
  const worldWithViewer = address
    ? `${worldBase}&address=${encodeURIComponent(address)}`
    : worldBase;
  const worldHref =
    role === "owner" ? worldWithViewer : `${worldWithViewer}&tab=layout`;
  return (
    <div className="chm__card" style={{ position: "relative" }}>
      <div className="chm__cardhead">
        <span className="chm__cardtitle u-truncate">{displayName}</span>
        <Link
          to={worldHref}
          prefetch="intent"
          onClick={() => onCardClick(card)}
          aria-label={displayName}
          style={{ position: "absolute", inset: 0, zIndex: 1 }}
        />
      </div>
      {!deployment ? (
        <EmptyState
          icon={undefined}
          subtitle={undefined}
          title="No scene published"
          actions={
            <Link
              to={href("/create/scenes")}
              prefetch="intent"
              className="btn btn--secondary btn--sm"
              style={{ textDecoration: "none", position: "relative", zIndex: 1 }}
              onClick={() => onCardClick(card)}
            >
              VIEW SCENES
            </Link>
          }
          variant={undefined}
          tone={undefined}
          actionsGap={undefined}
          style={{ flex: 1, "--es-pad": "24px 16px" } as CSSProperties}
        />
      ) : (
        <>
          <div className="chm__cardthumb">
            <div
              className="chm__thumbimg"
              style={
                deployment.thumbnail
                  ? {
                      backgroundImage: `url("${deployment.thumbnail}")`,
                      backgroundSize: "cover",
                      backgroundPosition: "center",
                    }
                  : { background: deployment.grad }
              }
            />
            <div className="chm__chips">
              {deployment.scenesCount > 0 ? (
                <span className="chm__chip">
                  {deployment.scenesCount}{" "}
                  {deployment.scenesCount === 1 ? "scene" : "scenes"}
                </span>
              ) : null}
              {roleLabel ? <span className="chm__chip">{roleLabel}</span> : null}
            </div>
          </div>
          <div className="chm__cardbody">
            <span className="chm__publabel">Published World</span>
            <span className="chm__projtitle">{deployment.title}</span>
            <Link
              to={worldHref}
              prefetch="intent"
              className="chm__settingsbtn"
              style={{ textDecoration: "none", position: "relative", zIndex: 1 }}
              onClick={() => onCardClick(card)}
            >
              {role === "owner" ? "Settings" : "Layout"}
            </Link>
            <Link
              to={`/creator-hub/scene-analytics?scene=${encodeURIComponent(
                `world:${card.id.toLowerCase()}`,
              )}&src=manage-card`}
              prefetch="intent"
              className="chm__settingsbtn"
              style={{ textDecoration: "none", position: "relative", zIndex: 1 }}
            >
              Analytics
            </Link>
          </div>
        </>
      )}
    </div>
  );
}

