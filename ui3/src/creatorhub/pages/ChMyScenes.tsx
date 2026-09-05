import { useMemo, useState, type JSX } from "react";
import { CreatorHubChromeMaybe } from "../frames/CreatorHubChrome";
import Dropdown from "../../components/Dropdown";
import "./chmyscenes.css";

export type MySceneVM = {
  entityId: string;
  kind: "land" | "world";
  title: string;
  baseParcel: string;
  pointers: string[];
  worldName: string | null;
  thumbnailUrl: string | null;
  editable: boolean;
  republishable?: boolean;
  deployedAt: number | null;
  openHref: string;
  syncState?: "local" | "local-ahead";
};

export type MySceneTab = { id: string; label: string; active?: boolean };

type SyncState = NonNullable<MySceneVM["syncState"]>;

const SYNC_META: Record<SyncState, { label: string; tone: string }> = {
  local: { label: "Saved locally", tone: "good" },
  "local-ahead": { label: "Edited locally", tone: "brand" },
};

const SORT_OPTIONS = [
  { value: "newest", label: "Newest" },
  { value: "oldest", label: "Oldest" },
  { value: "name", label: "Name" },
  { value: "kind", label: "Kind" },
] as const;
const SORT_LABELS = SORT_OPTIONS.map((o) => o.label);

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function normalizeTs(ts: number): number {
  return ts < 1e12 ? ts * 1000 : ts;
}

function relativeDeployed(ts: number | null): string | null {
  if (!ts) return null;
  const then = normalizeTs(ts);
  if (!Number.isFinite(then)) return null;
  const now = Date.now();
  const diff = now - then;
  const min = 60_000;
  const hour = 60 * min;
  const day = 24 * hour;
  if (diff < 0) {
    const d = new Date(then);
    return `Deployed ${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}`;
  }
  if (diff < hour) {
    const m = Math.max(1, Math.round(diff / min));
    return `Deployed ${m} min${m === 1 ? "" : "s"} ago`;
  }
  if (diff < day) {
    const h = Math.round(diff / hour);
    return `Deployed ${h} hour${h === 1 ? "" : "s"} ago`;
  }
  if (diff < 30 * day) {
    const d = Math.round(diff / day);
    return `Deployed ${d} day${d === 1 ? "" : "s"} ago`;
  }
  const d = new Date(then);
  return `Deployed ${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}`;
}

function hueFrom(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i += 1) h = (Math.imul(h, 31) + seed.charCodeAt(i)) >>> 0;
  return h % 360;
}

function shortAddress(addr: string): string {
  return addr && addr.length > 12 ? `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}` : addr;
}

function sceneSearchBlob(s: MySceneVM): string {
  return [s.title, s.baseParcel, s.worldName ?? "", s.pointers.join(" ")]
    .join(" ")
    .toLowerCase();
}

function sortedScenes(list: MySceneVM[], sortBy: string): MySceneVM[] {
  const at = (s: MySceneVM) => (s.deployedAt ? normalizeTs(s.deployedAt) : 0);
  const out = list.slice();
  if (sortBy === "oldest") {
    out.sort((a, b) => (at(a) || Infinity) - (at(b) || Infinity));
  } else if (sortBy === "name") {
    out.sort((a, b) => a.title.localeCompare(b.title) || at(b) - at(a));
  } else if (sortBy === "kind") {
    out.sort(
      (a, b) =>
        (a.kind === "world" ? 0 : 1) - (b.kind === "world" ? 0 : 1) || at(b) - at(a),
    );
  } else {
    out.sort((a, b) => at(b) - at(a));
  }
  return out;
}

const LandGlyph = () => (
  <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
    <rect x="3" y="3" width="14" height="14" rx="1.3" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <path d="M7.7 3v14M12.3 3v14M3 7.7h14M3 12.3h14" stroke="currentColor" strokeWidth="1.3" fill="none" />
  </svg>
);

const GlobeGlyph = () => (
  <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
    <circle cx="10" cy="10" r="7.2" stroke="currentColor" strokeWidth="1.4" fill="none" />
    <path d="M2.8 10h14.4M10 2.8c2 2 3 4.6 3 7.2s-1 5.2-3 7.2c-2-2-3-4.6-3-7.2s1-5.2 3-7.2Z" stroke="currentColor" strokeWidth="1.3" fill="none" />
  </svg>
);

const OpenGlyph = () => (
  <svg viewBox="0 0 20 20" width="17" height="17" aria-hidden="true">
    <path d="M4 13.5 13.5 4M8 4h5.5V9.5" stroke="currentColor" strokeWidth="1.7" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M13.5 11.5V15a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 15V8A1.5 1.5 0 0 1 5 6.5h3.5" stroke="currentColor" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

const DownloadGlyph = () => (
  <svg viewBox="0 0 20 20" width="17" height="17" aria-hidden="true">
    <path d="M10 3v9M6.2 8.8 10 12.5l3.8-3.7" stroke="currentColor" strokeWidth="1.7" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M4 14.5v1A1.5 1.5 0 0 0 5.5 17h9a1.5 1.5 0 0 0 1.5-1.5v-1" stroke="currentColor" strokeWidth="1.4" fill="none" strokeLinecap="round" />
  </svg>
);

const UploadGlyph = () => (
  <svg viewBox="0 0 20 20" width="17" height="17" aria-hidden="true">
    <path d="M10 12.5v-9M6.2 7.2 10 3.5l3.8 3.7" stroke="currentColor" strokeWidth="1.7" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M4 14.5v1A1.5 1.5 0 0 0 5.5 17h9a1.5 1.5 0 0 0 1.5-1.5v-1" stroke="currentColor" strokeWidth="1.4" fill="none" strokeLinecap="round" />
  </svg>
);

const SearchGlyph = () => (
  <svg viewBox="0 0 20 20" width="15" height="15" aria-hidden="true">
    <circle cx="9" cy="9" r="5.5" stroke="currentColor" strokeWidth="1.6" fill="none" />
    <path d="m13.2 13.2 3.6 3.6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);

const WalletGlyph = () => (
  <svg viewBox="0 0 44 44" width="44" height="44" aria-hidden="true">
    <rect x="6" y="11" width="32" height="24" rx="4" stroke="currentColor" strokeWidth="2.2" fill="none" />
    <path d="M6 17h32" stroke="currentColor" strokeWidth="2.2" />
    <circle cx="31" cy="26" r="2.4" fill="currentColor" />
  </svg>
);

const RadarGlyph = () => (
  <svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true">
    <circle cx="24" cy="24" r="6.5" stroke="currentColor" strokeWidth="2" fill="none" opacity="0.9" />
    <circle cx="24" cy="24" r="13" stroke="currentColor" strokeWidth="1.6" fill="none" opacity="0.45" />
    <circle cx="24" cy="24" r="19.5" stroke="currentColor" strokeWidth="1.4" fill="none" opacity="0.22" />
    <path d="M24 24 40 14" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
    <circle cx="24" cy="24" r="2.4" fill="currentColor" />
  </svg>
);

function StatusBadge({ state }: { state: SyncState }): JSX.Element {
  const meta = SYNC_META[state];
  return (
    <span
      className={`chms__badge chms__badge--${meta.tone}`}
      role="status"
      aria-label={`Status: ${meta.label}`}
    >
      <span className="chms__dot" aria-hidden="true" />
      {meta.label}
    </span>
  );
}

function PlaceholderTile({ scene }: { scene: MySceneVM }): JSX.Element {
  const seed = scene.kind === "world" ? scene.worldName ?? scene.entityId : scene.baseParcel || scene.entityId;
  const hue = hueFrom(seed);
  const label = scene.kind === "world" ? scene.worldName ?? "World" : scene.baseParcel || "\u{2014}";
  const style = {
    background: `radial-gradient(120% 120% at 22% 12%, hsl(${hue} 58% 34%), hsl(${(hue + 40) % 360} 46% 15%) 62%, #0b0a0e)`,
  } as const;
  return (
    <div className="chms__tile" style={style} aria-hidden="true">
      <span className="chms__tilekind">{scene.kind === "world" ? "WORLD" : "LAND"}</span>
      <span className="chms__tilecoord">{label}</span>
    </div>
  );
}

function SceneCard({
  scene,
  index,
  dupCount,
  downloading,
  onOpen,
  onDownload,
  onRepublish,
}: {
  scene: MySceneVM;
  index: number;
  dupCount: number;
  downloading: boolean;
  onOpen: (s: MySceneVM) => void;
  onDownload?: (s: MySceneVM) => void;
  onRepublish?: (s: MySceneVM) => void;
}): JSX.Element {
  const when = relativeDeployed(scene.deployedAt);
  const parcelCount = scene.kind === "land" ? scene.pointers.length : 0;
  const chipText =
    scene.kind === "world"
      ? `World: ${scene.worldName ?? "unnamed"}`
      : `LAND ${scene.baseParcel || "\u{2014}"}${parcelCount > 1 ? ` \u{B7} ${parcelCount} parcels` : ""}`;
  return (
    <article className="chms__card" style={{ animationDelay: `${Math.min(index, 12) * 45}ms` }}>
      <div className="chms__media">
        {scene.thumbnailUrl ? (
          <img className="chms__thumb" src={scene.thumbnailUrl} alt={`Preview of ${scene.title}`} loading="lazy" />
        ) : (
          <PlaceholderTile scene={scene} />
        )}
        {scene.syncState ? <StatusBadge state={scene.syncState} /> : null}
      </div>
      <div className="chms__body">
        <h3 className="chms__cardtitle u-truncate" title={scene.title}>
          {scene.title}
          {dupCount > 1 ? (
            <span
              className="chms__dup"
              title={`${dupCount} of your scenes share this name \u{2014} check the parcel or world to tell them apart`}
            >
              &#xD7;{dupCount}
            </span>
          ) : null}
        </h3>
        <div className="chms__meta">
          <span className={"chms__chip chms__chip--" + scene.kind}>
            <i className="chms__chipicon">{scene.kind === "world" ? <GlobeGlyph /> : <LandGlyph />}</i>
            <span className="u-truncate">{chipText}</span>
          </span>
        </div>
        {when ? <p className="chms__when">{when}</p> : <p className="chms__when chms__when--empty" />}
        <div className="chms__actions">
          {scene.editable ? (
            <div className="chms__stack">
              <button
                type="button"
                className="chms__btn chms__btn--primary"
                onClick={() => onOpen(scene)}
                aria-label={`Open ${scene.title} in the editor`}
              >
                <OpenGlyph />
                Open in editor
              </button>
              {scene.kind === "land" && scene.republishable !== false && onRepublish ? (
                <>
                  <button
                    type="button"
                    className="chms__btn chms__btn--ghost"
                    onClick={() => onRepublish(scene)}
                    aria-label={`Republish ${scene.title} to LAND ${scene.baseParcel}`}
                  >
                    <UploadGlyph />
                    Republish to LAND
                  </button>
                  <span className="chms__note">
                    Republishing updates {scene.baseParcel || "this LAND"} on this
                    network (catalyst.example.com) only &#x2014; Genesis City on decentraland.org is not
                    affected. Your wallet&apos;s LAND rights are checked before publish.
                  </span>
                </>
              ) : scene.kind === "land" ? (
                <span className="chms__note">
                  Publishing from here updates this network (catalyst.example.com) only, never
                  Genesis City on decentraland.org.
                </span>
              ) : null}
            </div>
          ) : (
            <div className="chms__viewonly">
              {onDownload ? (
                <button
                  type="button"
                  className="chms__btn chms__btn--ghost"
                  onClick={() => onDownload(scene)}
                  disabled={downloading}
                  aria-busy={downloading}
                  aria-label={`Download the deployed files of ${scene.title}`}
                >
                  <DownloadGlyph />
                  {downloading ? "Preparing zip\u{2026}" : "Download scene files"}
                </button>
              ) : null}
              {scene.kind === "land" && scene.republishable !== false && onRepublish ? (
                <button
                  type="button"
                  className="chms__btn chms__btn--ghost"
                  onClick={() => onRepublish(scene)}
                  aria-label={`Republish ${scene.title} to LAND ${scene.baseParcel}`}
                >
                  <UploadGlyph />
                  Republish to LAND
                </button>
              ) : null}
              <span className="chms__note">
                Code scene &#x2014; not editable here. Work on the files with the SDK CLI
                (<code className="chms__code">npx @dcl/sdk-commands start</code>).
                {scene.kind === "land"
                  ? " Republishing updates this network (catalyst.example.com) only \u{2014} Genesis City on decentraland.org is not affected."
                  : ""}
              </span>
            </div>
          )}
        </div>
      </div>
    </article>
  );
}

function SkeletonCard({ index }: { index: number }): JSX.Element {
  return (
    <div className="chms__card chms__card--skel" style={{ animationDelay: `${index * 70}ms` }} aria-hidden="true">
      <div className="chms__media chms__media--skel">
        <span className="chms__shimmer" />
      </div>
      <div className="chms__body">
        <span className="chms__skelline chms__skelline--title" />
        <span className="chms__skelline chms__skelline--chip" />
        <span className="chms__skelline chms__skelline--btn" />
      </div>
    </div>
  );
}

export default function ChMyScenes(props: {
  signedIn: boolean;
  account: string;
  name?: string;
  loading: boolean;
  scenes: MySceneVM[];
  onSignIn: () => void;
  onOpen: (s: MySceneVM) => void;
  onStartFromTemplate: () => void;
  onDownload?: (s: MySceneVM) => void;
  onRepublish?: (s: MySceneVM) => void;
  downloadingId?: string | null;
  tabs?: MySceneTab[];
  onTab?: (id: string) => void;
  chrome?: boolean;
}): JSX.Element {
  const {
    signedIn,
    account,
    name,
    loading,
    scenes,
    onSignIn,
    onOpen,
    onStartFromTemplate,
    onDownload,
    onRepublish,
    downloadingId,
    tabs,
    onTab,
    chrome = true,
  } = props;

  const [query, setQuery] = useState("");
  const [sortBy, setSortBy] = useState<string>("newest");

  const dupCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const s of scenes) {
      const key = s.title.trim().toLowerCase();
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  }, [scenes]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = q ? scenes.filter((s) => sceneSearchBlob(s).includes(q)) : scenes;
    return sortedScenes(filtered, sortBy);
  }, [scenes, query, sortBy]);

  let body: JSX.Element;

  if (!signedIn) {
    body = (
      <div className="chms__hero" role="region" aria-label="Sign in to see your scenes">
        <span className="chms__heroicon chms__heroicon--wallet" aria-hidden="true">
          <WalletGlyph />
        </span>
        <h1 className="chms__herotitle">Sign in to see your scenes</h1>
        <p className="chms__herolead">
          Your scenes will appear here once your wallet is connected.
        </p>
        <button type="button" className="chms__btn chms__btn--primary chms__btn--lg" onClick={onSignIn}>
          Sign in
        </button>
        <p className="chms__reassure">We only read your published scenes. Nothing is changed.</p>
      </div>
    );
  } else if (loading) {
    body = (
      <section className="chms__stage" aria-busy="true">
        <header className="chms__scanhead" role="status" aria-live="polite">
          <span className="chms__heroicon chms__heroicon--scan" aria-hidden="true">
            <RadarGlyph />
          </span>
          <div className="chms__scantext">
            <h1 className="chms__title">Finding scenes you&apos;ve deployed&#x2026;</h1>
            <p className="chms__subtitle">
              Reading from Decentraland&apos;s live content
              {account ? <> for <span className="chms__addr">{shortAddress(account)}</span></> : null}. Nothing is changed.
            </p>
          </div>
        </header>
        <div className="chms__grid" aria-hidden="true">
          {Array.from({ length: 6 }, (_, i) => (
            <SkeletonCard key={i} index={i} />
          ))}
        </div>
      </section>
    );
  } else if (scenes.length === 0) {
    body = (
      <div className="chms__hero" role="region" aria-label="No scenes found">
        <span className="chms__heroicon chms__heroicon--empty" aria-hidden="true">
          <RadarGlyph />
        </span>
        <h1 className="chms__herotitle">No published scenes found for this wallet yet</h1>
        <p className="chms__herolead">
          When you publish a scene to your LAND or a World, it&apos;ll show up here automatically &#x2014; ready
          to reopen and keep building. Start something new to get going.
        </p>
        <button type="button" className="chms__btn chms__btn--primary chms__btn--lg" onClick={onStartFromTemplate}>
          Start from a template
        </button>
      </div>
    );
  } else {
    const count = scenes.length;
    const filtering = query.trim().length > 0;
    body = (
      <section className="chms__stage">
        <header className="chms__head">
          <div className="chms__headmain">
            <h1 className="chms__title">Your published scenes</h1>
            <p className="chms__subtitle">
              {count} published scene{count === 1 ? "" : "s"} found
              {name ? <> for <span className="chms__addr">{name}</span></> : account ? <> for <span className="chms__addr">{shortAddress(account)}</span></> : null}
              . Opening one loads your deployed copy &#x2014; edits stay in this browser until you save
              them to disk.
            </p>
          </div>
          <button type="button" className="chms__btn chms__btn--ghost" onClick={onStartFromTemplate}>
            Start from a template
          </button>
        </header>
        <div className="chms__toolbar">
          <label className="chms__search">
            <span className="chms__searchicon" aria-hidden="true">
              <SearchGlyph />
            </span>
            <input
              type="search"
              className="chms__searchinput"
              placeholder="Search by name, parcel or world"
              aria-label="Search your published scenes"
              value={query}
              onChange={(e) => setQuery(e.currentTarget.value)}
            />
          </label>
          <div className="chms__sortwrap">
            <span className="chms__sortlabel">Sort by</span>
            <Dropdown
              options={SORT_LABELS}
              value={(SORT_OPTIONS.find((o) => o.value === sortBy) ?? SORT_OPTIONS[0]).label}
              onChange={(label: string) => {
                const opt = SORT_OPTIONS.find((o) => o.label === label);
                if (opt) setSortBy(opt.value);
              }}
            />
          </div>
          {filtering ? (
            <span className="chms__matchcount" role="status" aria-live="polite">
              {visible.length} of {count} match
            </span>
          ) : null}
        </div>
        {visible.length === 0 ? (
          <p className="chms__nomatch" role="status">
            No scenes match &quot;{query.trim()}&quot;.
          </p>
        ) : (
          <div className="chms__grid">
            {visible.map((scene, i) => (
              <SceneCard
                key={scene.entityId}
                scene={scene}
                index={i}
                dupCount={dupCounts.get(scene.title.trim().toLowerCase()) ?? 1}
                downloading={downloadingId === scene.entityId}
                onOpen={onOpen}
                onDownload={onDownload}
                onRepublish={onRepublish}
              />
            ))}
          </div>
        )}
      </section>
    );
  }

  return (
    <CreatorHubChromeMaybe chrome={chrome} active="scenes" signedIn={signedIn} account={account} name={name} onSignIn={onSignIn}>
      <div className="chms">
        {tabs && tabs.length > 0 ? (
          <nav className="chms__tabs" aria-label="Scene lists">
            {tabs.map((t) => (
              <button
                key={t.id}
                type="button"
                className={"chms__tab" + (t.active ? " is-active" : "")}
                aria-current={t.active ? "page" : undefined}
                onClick={() => onTab?.(t.id)}
              >
                {t.label}
              </button>
            ))}
          </nav>
        ) : null}
        {body}
      </div>
    </CreatorHubChromeMaybe>
  );
}
