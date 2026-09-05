import type { CSSProperties } from "react";
import { useMemo, useState } from "react";

import ExploreChrome, { type TabId } from "../frames/ExploreChrome";
import SearchField from "../../atoms/SearchField";
import { fmt, hueFromSeed } from "../../atoms/primitives";
import "./communities.css";

type Community = {
  id?: string;
  name?: string;
  thumbnailUrl?: string;
  visibility?: string | null;
  privacy?: string;
  isLive?: boolean;
  membersCount: number;
  role?: string | null;
  ownerName?: string;
};

const isHttpUrl = (s: unknown): boolean => typeof s === "string" && /^https?:\/\//i.test(s);

function bannerStyle(c: Community): CSSProperties & { "--hue": number } {
  const hue = hueFromSeed(c.id || c.name || "");
  if (isHttpUrl(c.thumbnailUrl)) {
    return {
      "--hue": hue,
      backgroundImage: `url("${c.thumbnailUrl}")`,
      backgroundSize: "cover",
      backgroundPosition: "center",
    };
  }
  return { "--hue": hue };
}

function visLabel(c: Community): string {
  if (c.visibility === "unlisted") return "Unlisted";
  return c.privacy === "private" ? "Private" : "Public";
}

function Card({ c }: { c: Community }) {
  const isPublic = c.privacy !== "private";
  const hasThumb = isHttpUrl(c.thumbnailUrl);
  const joined = Boolean(c.role) && c.role !== "none";
  return (
    <article className="cm__card" data-sb-linkto="Explorer/Pages/CommunityCard">
      <div className="cm__banner" style={bannerStyle(c)}>
        {c.isLive ? (
          <span
            aria-hidden="true"
            style={{
              position: "absolute", top: 10, left: 10, padding: "3px 8px",
              borderRadius: 999, background: "rgba(0,0,0,.55)", color: "#ff5a7a",
              fontSize: 11, fontWeight: 800, letterSpacing: ".04em",
            }}
          >
            &#x25CF; LIVE
          </span>
        ) : null}
        {hasThumb ? null : (
          <span className="cm__bannermark" aria-hidden="true">{c.name}</span>
        )}
      </div>
      <div className="cm__cardbody">
        <div className="cm__name u-truncate" title={c.name}>{c.name}</div>
        <div className="cm__meta">
          <span className="cm__vispill">
            <span className="cm__visicon" aria-hidden="true">{isPublic ? "\u{1F310}" : "\u{1F512}"}</span>
            <span className={"cm__vis" + (isPublic ? " is-public" : "")}>{visLabel(c)}</span>
          </span>
          <span className="cm__memberchip">
            <span className="cm__membicon" aria-hidden="true">&#x1F464;</span>
            {fmt(c.membersCount)} Members
          </span>
        </div>
        <div className="cm__actions">
          {joined ? (
            <button className="cm__joined" type="button">
              Joined
              <span className="cm__btncoin" aria-hidden="true">&#x25C6;</span>
            </button>
          ) : (
            <button className="cm__join" type="button">
              {isPublic ? "Join" : "Request to join"}
              <span className="cm__btncoin" aria-hidden="true">&#x25C6;</span>
            </button>
          )}
        </div>
      </div>
    </article>
  );
}

const BULLETS = [
  { icon: "\u{2665}", text: "Connect over shared interests" },
  { icon: "\u{2691}", text: "Get notified about community events" },
  { icon: "\u{2726}", text: "Chat in a shared channel" },
];

type SidebarProps = {
  mine: Community[];
  showingMine: boolean;
  onToggleMine: () => void;
};

function Sidebar({ mine, showingMine, onToggleMine }: SidebarProps) {
  const crewA: CSSProperties & { "--hue": number } = { "--hue": 280, left: "16%" };
  const crewB: CSSProperties & { "--hue": number } = { "--hue": 50, left: "42%" };
  const crewC: CSSProperties & { "--hue": number } = { "--hue": 0, left: "68%" };
  return (
    <aside className="cm__side">
      <button className="cm__create" type="button" data-sb-linkto="Explorer/Components/CommunityCreate">+ Create a Community</button>

      <button className="cm__invites" type="button">
        <span>Invites &amp; Requests</span>
      </button>

      <div className="cm__minesection">
        <div className="cm__minehead">
          <span className="cm__minelabel">My Communities</span>
          {mine.length > 0 && (
            <button className="cm__mineviewall" type="button" onClick={onToggleMine}>
              {showingMine ? "\u{2039} BACK" : "VIEW ALL \u{203A}"}
            </button>
          )}
        </div>
        {mine.length === 0 ? (
          <span className="cm__mineempty">You haven&#x2019;t joined any communities yet.</span>
        ) : (
          mine.map((c) => (
            <div className="cm__minerow" key={c.id} data-sb-linkto="Explorer/Pages/CommunityMembers">
              <span className="u-avatar" style={{ "--sz": "32px", "--hue": hueFromSeed(c.id || c.name || "") } as CSSProperties} />
              <span className="cm__minename u-truncate" title={c.name}>{c.name}</span>
            </div>
          ))
        )}
      </div>

      <div className="cm__promo">
        <div className="cm__promotitle">
          <span className="cm__promospark" aria-hidden="true">&#x2726;</span>
          Discover Communities!
        </div>
        <div className="cm__promosub">Find your people. Join the fun.</div>

        <div className="cm__promoart" aria-hidden="true">
          <span className="cm__crew" style={crewA} />
          <span className="cm__crew" style={crewB} />
          <span className="cm__crew" style={crewC} />
          <span className="cm__fire" />
        </div>

        <ul className="cm__bullets">
          {BULLETS.map((b) => (
            <li className="cm__bullet" key={b.text}>
              <span className="cm__bicon" aria-hidden="true">{b.icon}</span>
              {b.text}
            </li>
          ))}
        </ul>
      </div>
    </aside>
  );
}

type CommunitiesProps = {
  communities?: Community[] | null;
  isLoading?: boolean;
  isError?: boolean;
  error?: { message?: string } | null;
};

function isMember(c: Community): boolean {
  return Boolean(c.role) && c.role !== "none";
}

export default function Communities({
  communities = [],
  isLoading = false,
  isError = false,
  error = null,
}: CommunitiesProps) {
  const [tab, setTab] = useState<TabId>("communities");
  const [query, setQuery] = useState("");
  const [mineTab, setMineTab] = useState<"browse" | "mine">("browse");

  const all = communities ?? [];
  const mine = useMemo(() => all.filter(isMember), [all]);
  const matches = (c: Community, q: string) =>
    !q || (c.name || "").toLowerCase().includes(q) || (c.ownerName || "").toLowerCase().includes(q);
  const q = query.trim().toLowerCase();
  const filtered = useMemo(() => all.filter((c) => matches(c, q)), [all, q]);
  const mineFiltered = useMemo(() => mine.filter((c) => matches(c, q)), [mine, q]);
  const showingMine = mineTab === "mine";
  const gridList = showingMine ? mineFiltered : filtered;

  let body;
  if (isLoading) {
    body = (
      <div className="cm__grid" aria-busy="true">
        {Array.from({ length: 8 }).map((_, i) => {
          const bannerLoadingStyle: CSSProperties & { "--hue": number } = {
            "--hue": (i * 36) % 360,
            opacity: 0.5,
          };
          return (
          <article className="cm__card" key={i}>
            <div className="cm__banner" style={bannerLoadingStyle} />
            <div className="cm__cardbody"><div className="cm__name">Loading&#x2026;</div></div>
          </article>
          );
        })}
      </div>
    );
  } else if (isError) {
    body = (
      <div className="cm__seclabel" role="alert">
        Couldn&#x2019;t load communities{error?.message ? ` \u{2014} ${error.message}` : ""}.
      </div>
    );
  } else if (gridList.length === 0) {
    body = (
      <div className="cm__seclabel">
        {showingMine
          ? "You haven\u{2019}t joined any communities yet."
          : query
            ? "No communities match your search."
            : "No communities found."}
      </div>
    );
  } else {
    body = (
      <div className="cm__grid">
        {gridList.map((c) => <Card key={c.id} c={c} />)}
      </div>
    );
  }

  return (
    <ExploreChrome active={tab} onTab={setTab}>
      <div className="cm">
        <Sidebar
          mine={mine}
          showingMine={showingMine}
          onToggleMine={() => setMineTab((t) => (t === "mine" ? "browse" : "mine"))}
        />

        <section className="cm__main">
          <div className="cm__header">
            <h1 className="cm__title">Communities</h1>
            <div className="cm__headactions">
              <button className="cm__createtop" type="button" data-sb-linkto="Explorer/Components/CommunityCreate">
                + Create a Community
              </button>
              <div className="cm__searchwrap">
                <SearchField placeholder="Search" value={query} onChange={setQuery} />
              </div>
            </div>
          </div>

          <div className="cm__section">
            <div className="cm__seclabel">
              {showingMine ? "My Communities" : "Browse Communities"} {isLoading ? "" : `(${gridList.length})`}
            </div>
            {body}
          </div>
        </section>
      </div>
    </ExploreChrome>
  );
}
