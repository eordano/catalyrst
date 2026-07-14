import type { CSSProperties } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ExploreChrome, { type TabId } from "../frames/ExploreChrome";
import SearchField from "../../atoms/SearchField";
import PlaceCard from "../../components/PlaceCard";
import Dropdown from "../../components/Dropdown";
import "./places.css";

type Section = { id: string; label: string; icon: string };
type Category = { id: string; label: string; icon: string; c: string };
type PlaceCardData = {
  id?: string;
  title: string;
  creator: string;
  players: number | null;
  rating: number;
  coords?: string;
  live?: number;
  featured?: boolean;
  to?: string;
  image?: string;
  categories?: string[];
};

const LIVE_NOW_LIMIT = 4;

const SORT_OPTIONS: { label: string; value: string }[] = [
  { label: "Most active", value: "most_active" },
  { label: "Best rated", value: "like_score" },
  { label: "Newest", value: "created_at" },
];
const DEFAULT_SORT = { label: "Most active", value: "most_active" };

function cardIdentity(c: PlaceCardData): string {
  return c.id ?? c.title;
}

function cardKey(c: PlaceCardData, i: number): string {
  return `${cardIdentity(c)}-${i}`;
}

function Card({ card, index }: { card: PlaceCardData; index: number }) {
  return (
    <div className="pl__cardwrap" data-place-id={cardIdentity(card)}>
      <PlaceCard
        title={card.title}
        image={card.image}
        players={card.players}
        rating={card.rating}
        coords={card.coords}
        live={card.live}
        featured={card.featured}
        creator={card.creator}
        hue={(index * 47) % 360}
        to={card.to}
      />
    </div>
  );
}

function Chevron({ dir }: { dir: "left" | "right" }) {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor"
      strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={dir === "left" ? "M15 18l-6-6 6-6" : "M9 18l6-6-6-6"} />
    </svg>
  );
}

function FeaturedCarousel({ cards }: { cards: PlaceCardData[] }) {
  const track = useRef<HTMLDivElement>(null);
  const [canPrev, setCanPrev] = useState(false);
  const [canNext, setCanNext] = useState(false);

  const sync = useCallback(() => {
    const el = track.current;
    if (!el) return;
    setCanPrev(el.scrollLeft > 1);
    setCanNext(el.scrollLeft < el.scrollWidth - el.clientWidth - 1);
  }, []);

  useEffect(() => {
    sync();
    const el = track.current;
    el?.addEventListener("scroll", sync, { passive: true });
    window.addEventListener("resize", sync);
    return () => {
      el?.removeEventListener("scroll", sync);
      window.removeEventListener("resize", sync);
    };
  }, [sync, cards]);

  const page = (dir: 1 | -1) => {
    const el = track.current;
    if (el) el.scrollBy({ left: dir * el.clientWidth * 0.85, behavior: "smooth" });
  };

  return (
    <div className="pl__carousel">
      {canPrev && (
        <button type="button" className="pl__carouselarrow pl__carouselarrow--prev" aria-label="Previous" onClick={() => page(-1)}>
          <Chevron dir="left" />
        </button>
      )}
      <div className="pl__carouseltrack" ref={track}>
        {cards.map((card, i) => (
          <div className="pl__carouselitem" key={cardKey(card, i)}>
            <Card card={card} index={i} />
          </div>
        ))}
      </div>
      {canNext && (
        <button type="button" className="pl__carouselarrow pl__carouselarrow--next" aria-label="Next" onClick={() => page(1)}>
          <Chevron dir="right" />
        </button>
      )}
    </div>
  );
}

const SECTIONS: Section[] = [
  { id: "explore", label: "Explore", icon: "explore" },
  { id: "recent", label: "Recent", icon: "recent" },
  { id: "favorites", label: "Favorites", icon: "heart" },
  { id: "myplaces", label: "My Places", icon: "pin" },
];

const CATEGORIES: Category[] = [
  { id: "all", label: "All", icon: "grid", c: "#3b7bff" },
  { id: "social", label: "Social", icon: "social", c: "#3b9dff" },
  { id: "music", label: "Music", icon: "music", c: "#27c06a" },
  { id: "art", label: "Art", icon: "art", c: "#19c8b0" },
  { id: "game", label: "Game", icon: "game", c: "#c86bff" },
  { id: "fashion", label: "Fashion", icon: "fashion", c: "#ff4d8d" },
  { id: "education", label: "Education", icon: "education", c: "#ffb02e" },
  { id: "shop", label: "Shop", icon: "shop", c: "#ff5da2" },
  { id: "sports", label: "Sports", icon: "sports", c: "#37c46b" },
  { id: "business", label: "Business", icon: "business", c: "#7b8cff" },
];

const PILL_ICONS: Record<string, string> = {
  explore: "M12 3l2.4 5.6L20 11l-5.6 2.4L12 19l-2.4-5.6L4 11l5.6-2.4L12 3Z",
  recent: "M12 7v5l3 2M4 12a8 8 0 1 0 8-8",
  heart: "M12 19s-7-4.4-7-9.3A3.6 3.6 0 0 1 12 7a3.6 3.6 0 0 1 7-1.3C19 14.6 12 19 12 19Z",
  pin: "M12 21s7-6.2 7-11a7 7 0 1 0-14 0c0 4.8 7 11 7 11Z",
};

const CAT_ICONS: Record<string, string> = {
  grid: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
  social: "M9 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM3 20c0-3.3 2.7-5 6-5s6 1.7 6 5M16 6a3 3 0 0 1 0 6",
  music: "M9 18V6l10-2v12M9 18a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Zm10-2a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Z",
  art: "M12 3a9 9 0 1 0 0 18c1.5 0 2-1 2-2s-1-2 0-3 3 0 4-2a8 8 0 0 0-6-9ZM7 11a1 1 0 1 0 0-2M11 8a1 1 0 1 0 0-2",
  game: "M7 9h10a4 4 0 0 1 4 4v1a3 3 0 0 1-5.2 2H8.2A3 3 0 0 1 3 14v-1a4 4 0 0 1 4-4ZM6 12h3m-1.5-1.5v3",
  fashion: "M9 4l3 3 3-3 4 4-3 2v9H8v-9l-3-2 4-4Z",
  education: "M3 9l9-4 9 4-9 4-9-4ZM7 11v5c0 1.1 2.2 2 5 2s5-.9 5-2v-5",
  shop: "M5 8h14l-1 11H6L5 8ZM9 8a3 3 0 0 1 6 0",
  sports: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm0 0v18M3 12h18M5 6c3 2 11 2 14 0M5 18c3-2 11-2 14 0",
  business: "M4 8h16v12H4zM9 8V5h6v3M4 13h16",
};

const skeletonStyle = (i: number): CSSProperties & { "--hue": number } => ({
  "--hue": (i * 47) % 360,
  opacity: 0.5,
});

type PlacesProps = {
  places?: PlaceCardData[];
  categories?: Category[];
  loading?: boolean;
  error?: boolean;
  section?: string;
  onSectionChange?: (id: string) => void;
  sort?: string;
  onSortChange?: (orderBy: string) => void;
};

export default function Places({
  places = [],
  categories = CATEGORIES,
  loading = false,
  error = false,
  section: sectionProp,
  onSectionChange,
  sort: sortProp,
  onSortChange,
}: PlacesProps) {
  const [tab, setTab] = useState<TabId>("places");
  const [internalSection, setInternalSection] = useState("explore");
  const section = sectionProp ?? internalSection;
  const setSection = onSectionChange ?? setInternalSection;
  const [internalSort, setInternalSort] = useState("most_active");
  const sort = sortProp ?? internalSort;
  const setSort = onSortChange ?? setInternalSort;
  const [cat, setCat] = useState("all");
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const shown = places.filter((p) => {
    const okQ = !q || `${p.title} ${p.creator}`.toLowerCase().includes(q);
    const okCat = cat === "all" || (p.categories ?? []).some((c) => c.toLowerCase() === cat);
    return okQ && okCat;
  });

  const showHighlights = section === "explore" && cat === "all" && q === "" && !loading && !error;
  const liveNow = useMemo(() => {
    if (!showHighlights) return [];
    return [...places]
      .filter((p) => (p.live ?? 0) > 0)
      .sort((a, b) => (b.live ?? 0) - (a.live ?? 0))
      .slice(0, LIVE_NOW_LIMIT);
  }, [places, showHighlights]);
  const liveIds = useMemo(() => new Set(liveNow.map(cardIdentity)), [liveNow]);
  const featured = useMemo(() => {
    if (!showHighlights) return [];
    return places.filter((p) => p.featured && !liveIds.has(cardIdentity(p)));
  }, [places, showHighlights, liveIds]);
  const featuredIds = useMemo(() => new Set(featured.map(cardIdentity)), [featured]);
  const mainList = showHighlights
    ? shown.filter((p) => !liveIds.has(cardIdentity(p)) && !featuredIds.has(cardIdentity(p)))
    : shown;

  return (
    <ExploreChrome active={tab} onTab={setTab}>
      <div className="pl">
        <div className="pl__head">
          <h1 className="pl__title">Places</h1>

          <div className="pl__sections" role="tablist" aria-label="Places sections">
            {SECTIONS.map((s) => (
              <button
                key={s.id}
                type="button"
                role="tab"
                aria-selected={s.id === section}
                className={"pl__sectab" + (s.id === section ? " is-active" : "")}
                onClick={() => setSection(s.id)}
              >
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none"
                  stroke="currentColor" strokeWidth="2" strokeLinecap="round"
                  strokeLinejoin="round" aria-hidden="true">
                  <path d={PILL_ICONS[s.icon]} />
                </svg>
                {s.label}
              </button>
            ))}
          </div>

          <div className="pl__actions">
            <div className="pl__search"><SearchField placeholder="Search places & worlds" value={query} onChange={setQuery} /></div>
            {section !== "recent" && (
              <div className="pl__sort">
                <Dropdown
                  ariaLabel="Sort places"
                  options={SORT_OPTIONS.map((o) => o.label)}
                  value={SORT_OPTIONS.find((o) => o.value === sort)?.label ?? DEFAULT_SORT.label}
                  onChange={(label) => setSort(SORT_OPTIONS.find((o) => o.label === label)?.value ?? DEFAULT_SORT.value)}
                />
              </div>
            )}
          </div>
        </div>

        <div className="pl__cats" role="tablist" aria-label="Place categories">
          {categories.map((c) => (
            <button
              key={c.id}
              type="button"
              role="tab"
              aria-selected={c.id === cat}
              className={"pl__pill" + (c.id === cat ? " is-active" : "")}
              onClick={() => setCat(c.id)}
            >
              <span className="pl__pillicon" style={{ background: c.c }}>
                <svg viewBox="0 0 24 24" width="12" height="12" fill="none"
                  stroke="#fff" strokeWidth="2" strokeLinecap="round"
                  strokeLinejoin="round" aria-hidden="true">
                  <path d={CAT_ICONS[c.icon]} />
                </svg>
              </span>
              {c.label}
            </button>
          ))}
        </div>

        {showHighlights && liveNow.length > 0 && (
          <section className="pl__highlight">
            <h2 className="pl__highlighttitle"><span className="pl__livedot" aria-hidden="true" /> Live Now</h2>
            <div className="pl__highlightrow">
              {liveNow.map((card, i) => <Card card={card} index={i} key={cardKey(card, i)} />)}
            </div>
          </section>
        )}

        {showHighlights && featured.length > 0 && (
          <section className="pl__highlight">
            <h2 className="pl__highlighttitle">Featured Places</h2>
            <FeaturedCarousel cards={featured} />
          </section>
        )}

        <div className="pl__grid" aria-busy={loading || undefined}>
          {loading ? (
            Array.from({ length: 8 }).map((_, i) => (
              <article className="pl__card" key={"sk" + i} aria-hidden="true">
                <div className="pl__thumb" style={skeletonStyle(i)} />
              </article>
            ))
          ) : error ? (
            <div className="pl__statemsg" role="alert" style={{ gridColumn: "1 / -1", padding: "32px 8px", color: "rgba(255,255,255,.6)" }}>
              Couldn&#x2019;t load places. Please try again.
            </div>
          ) : shown.length === 0 ? (
            <div className="pl__statemsg" style={{ gridColumn: "1 / -1", padding: "32px 8px", color: "rgba(255,255,255,.6)" }}>
              No places found.
            </div>
          ) : (
            mainList.map((card, i) => <Card card={card} index={i} key={cardKey(card, i)} />)
          )}
        </div>
      </div>
    </ExploreChrome>
  );
}
