import type { CSSProperties, ReactNode } from "react";
import { useState } from "react";
import Spinner from "../../atoms/Spinner";
import "./chitemdetail.css";
import { ChevronLeft } from "../../atoms/icons";

type MetricKind =
  | "triangles"
  | "materials"
  | "textures"
  | "sequences"
  | "duration"
  | "frames"
  | "fps";

interface Metrics {
  triangles: number;
  materials: number;
  textures: number;
  sequences?: number;
  duration?: number;
  frames?: number;
  fps?: number;
}

interface Representation {
  mainFile: string;
  bodyShape: string;
}

interface Item {
  id?: string;
  type: string;
  name: string;
  description: string;
  utility: string;
  rarity: string;
  category: string;
  bodyShape: string;
  smart: boolean;
  isPublished: boolean;
  tokenId: string | null;
  totalSupply?: number;
  maxSupply?: number;
  collection: string;
  urn: string;
  price: string;
  beneficiary: string;
  hue: number;
  tags: string[];
  metrics: Metrics;
  representations: Representation[];
  requiredPermissions: string[];
}

const RARITY: Record<string, string> = {
  common: "--rar-common",
  uncommon: "--rar-uncommon",
  rare: "--rar-rare",
  epic: "--rar-epic",
  legendary: "--rar-legendary",
  mythic: "--rar-mythic",
  unique: "--rar-unique",
  exotic: "--rar-exotic",
};

const EMPTY_ITEM: Item = {
  type: "wearable",
  name: "",
  description: "",
  utility: "",
  rarity: "",
  category: "",
  bodyShape: "",
  smart: false,
  isPublished: false,
  tokenId: null,
  collection: "",
  urn: "",
  price: "",
  beneficiary: "",
  hue: 0,
  tags: [],
  metrics: { triangles: 0, materials: 0, textures: 0 },
  representations: [],
  requiredPermissions: [],
};

function MetricGlyph({ kind }: { kind: MetricKind }) {
  const p = {
    triangles: <path d="M8 2l6 11H2L8 2z" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />,
    materials: (
      <>
        <rect x="2.5" y="2.5" width="6" height="6" rx="1" fill="none" stroke="currentColor" strokeWidth="1.2" />
        <rect x="7.5" y="7.5" width="6" height="6" rx="1" fill="none" stroke="currentColor" strokeWidth="1.2" />
      </>
    ),
    textures: (
      <>
        <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.2" />
        <path d="M2.5 10l3-3 3 3 2-2 3 3" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
        <circle cx="6" cy="6" r="1" fill="currentColor" />
      </>
    ),
    sequences: (
      <>
        <rect x="2.5" y="2.5" width="4.5" height="4.5" rx="1" fill="none" stroke="currentColor" strokeWidth="1.2" />
        <rect x="9" y="2.5" width="4.5" height="4.5" rx="1" fill="none" stroke="currentColor" strokeWidth="1.2" />
        <rect x="2.5" y="9" width="4.5" height="4.5" rx="1" fill="none" stroke="currentColor" strokeWidth="1.2" />
        <rect x="9" y="9" width="4.5" height="4.5" rx="1" fill="none" stroke="currentColor" strokeWidth="1.2" />
      </>
    ),
    duration: (
      <>
        <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.2" />
        <path d="M8 4.6V8l2.4 1.4" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
      </>
    ),
    frames: (
      <>
        <rect x="2.5" y="3.5" width="11" height="9" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.2" />
        <path d="M6.5 6.2l3.5 1.8-3.5 1.8V6.2z" fill="currentColor" />
      </>
    ),
    fps: (
      <>
        <rect x="2.5" y="3.5" width="11" height="9" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.2" />
        <path d="M5.5 8h5M8 5.5v5" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      </>
    ),
  }[kind];
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      {p}
    </svg>
  );
}

const CameraGlyph = () => (
  <svg viewBox="0 0 18 18" width="16" height="16" aria-hidden="true">
    <path
      d="M2.5 5.5h2.2l1-1.6h4.6l1 1.6h2.2a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1H2.5a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1z"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.3"
      strokeLinejoin="round"
    />
    <circle cx="9" cy="9.6" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.3" />
  </svg>
);
const CopyGlyph = () => (
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.4" fill="none" stroke="currentColor" strokeWidth="1.3" />
    <path d="M3.5 10.5h-1V3a.5.5 0 0 1 .5-.5h7.5v1" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const DotsGlyph = () => (
  <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
    <circle cx="3" cy="8" r="1.4" fill="currentColor" />
    <circle cx="8" cy="8" r="1.4" fill="currentColor" />
    <circle cx="13" cy="8" r="1.4" fill="currentColor" />
  </svg>
);
const ManaGlyph = ({ size = 16 }) => (
  <svg viewBox="0 0 16 16" width={size} height={size} aria-hidden="true">
    <path d="M8 1.6L13 8 8 14.4 3 8 8 1.6z M8 4.4L5 8l3 3.6L11 8 8 4.4z" fill="currentColor" />
  </svg>
);
function BodyBadge({
  smart,
  bodyShape,
  floating,
}: {
  smart?: boolean;
  bodyShape: string;
  floating?: boolean;
}) {
  const cls = "bditemdetail__badge" + (floating ? " bditemdetail__badge--float" : "");
  if (smart) {
    return (
      <span className={cls + " is-smart"} title="Smart Wearable">
        <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
          <path d="M9 1.5L3 9h4l-1 5.5L13 7H9l1-5.5z" fill="currentColor" />
        </svg>
      </span>
    );
  }
  const glyph =
    ({
      male: <path d="M9.5 6.5l3-3m0 0h-2.5m2.5 0v2.5M8 7.5a3 3 0 1 0 0 6 3 3 0 0 0 0-6z" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />,
      female: <path d="M8 2.5a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM8 8.5v5M6 11.5h4" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />,
      both: <path d="M5 8.5a2.4 2.4 0 1 0 0-4.8 2.4 2.4 0 0 0 0 4.8zM11 8.5a2.4 2.4 0 1 0 0-4.8 2.4 2.4 0 0 0 0 4.8zM5 8.5v4M3.5 11h3M9.5 6.1l3-3" fill="none" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" strokeLinejoin="round" />,
    } as Record<string, ReactNode>)[bodyShape] || null;
  return (
    <span className={cls} title={bodyShape}>
      <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
        {glyph}
      </svg>
    </span>
  );
}

function Properties({ item }: { item: Item }) {
  const isEmote = item.type === "emote";
  const m = item.metrics;
  const rows: { kind: MetricKind; label: string }[] = isEmote
    ? [
        { kind: "sequences", label: `${m.sequences} ${m.sequences === 1 ? "sequence" : "sequences"}` },
        { kind: "duration", label: `${m.duration} ${m.duration === 1 ? "second" : "seconds"}` },
        { kind: "frames", label: `${m.frames} ${m.frames === 1 ? "frame" : "frames"}` },
        { kind: "fps", label: `${m.fps} fps` },
      ]
    : [
        { kind: "triangles", label: `${m.triangles.toLocaleString()} ${m.triangles === 1 ? "triangle" : "triangles"}` },
        { kind: "materials", label: `${m.materials} ${m.materials === 1 ? "material" : "materials"}` },
        { kind: "textures", label: `${m.textures} ${m.textures === 1 ? "texture" : "textures"}` },
      ];
  return (
    <div className="bditemdetail__metrics">
      <span className="bditemdetail__subtitle">Properties</span>
      <div className="bditemdetail__metriclist">
        {rows.map((r) => (
          <div key={r.kind} className="bditemdetail__metric">
            <MetricGlyph kind={r.kind} />
            {r.label}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function ChItemDetail({
  item = EMPTY_ITEM,
  loading = false,
  bare: _bare = false,
  onBack = undefined,
}: {
  item?: Item;
  loading?: boolean;
  bare?: boolean;
  onBack?: () => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);

  if (loading) {
    const loadingBody = (
      <div className="bditemdetail bditemdetail--loading">
        <Spinner size={48} />
      </div>
    );
    return loadingBody;
  }

  const isSmart = item.smart;
  const isWearable = item.type === "wearable";
  const showRepresentations = isWearable && !isSmart && item.representations.length > 0;
  const showPermissions = isSmart && item.requiredPermissions.length > 0;
  const rarToken = RARITY[item.rarity];

  const body = (
      <div className="bditemdetail">
        <div className="bditemdetail__container">
          {onBack ? (
            <button
              type="button"
              className="bditemdetail__back"
              aria-label="Back"
              onClick={onBack}
            >
              <ChevronLeft size={18} />
            </button>
          ) : null}

          <div className="bditemdetail__data">
            <div className="bditemdetail__left">
              <div
                className="bditemdetail__image u-rar-bg"
                style={
                  {
                    "--rb": `var(${"--rar-bg-" + item.rarity})`,
                    background: `linear-gradient(155deg, hsl(${item.hue} 62% 42%), hsl(${(item.hue + 40) % 360} 55% 22%))`,
                  } as CSSProperties & { "--rb": string }
                }
              >
                {item.rarity ? (
                  <span className="bditemdetail__rarpill" style={{ background: `var(${rarToken})` }}>
                    {item.rarity}
                  </span>
                ) : null}
                <BodyBadge smart={item.smart} bodyShape={item.bodyShape} floating />
              </div>

              <button
                type="button"
                className="bditemdetail__thumbbtn"
                disabled
                title="Not available on this realm yet"
              >
                <CameraGlyph /> Edit Thumbnail
              </button>

              <Properties item={item} />

              <div className="bditemdetail__details">
                <span className="bditemdetail__subtitle bditemdetail__detailshead">Details</span>
                {item.isPublished ? (
                  <div className="bditemdetail__detailrow">
                    <span className="bditemdetail__subtitle">Id</span>
                    <span className="bditemdetail__value">#{item.tokenId}</span>
                  </div>
                ) : null}
                {item.category ? (
                  <div className="bditemdetail__detailrow">
                    <span className="bditemdetail__subtitle">Category</span>
                    <span className="bditemdetail__value">{item.category.replace(/_/g, " ")}</span>
                  </div>
                ) : null}
                {item.isPublished && item.rarity ? (
                  <div className="bditemdetail__detailrow">
                    <span className="bditemdetail__subtitle">Supply</span>
                    <span className="bditemdetail__value">
                      {item.totalSupply}/{item.maxSupply}
                    </span>
                  </div>
                ) : null}
                {item.collection ? (
                  <div className="bditemdetail__detailrow">
                    <span className="bditemdetail__subtitle">Collection</span>
                    <button type="button" className="bditemdetail__collink">
                      {item.collection}
                    </button>
                  </div>
                ) : null}
                {item.urn ? (
                  <div className="bditemdetail__detailrow">
                    <span className="bditemdetail__subtitle">URN</span>
                    <span className="bditemdetail__value bditemdetail__urn">
                      <span>
                        {item.urn}
                        <button
                          type="button"
                          className="bditemdetail__copy"
                          aria-label="Copy urn"
                          onClick={() => {
                            void navigator.clipboard?.writeText(item.urn);
                          }}
                        >
                          <CopyGlyph />
                        </button>
                      </span>
                    </span>
                  </div>
                ) : null}
              </div>
            </div>

            <div className="bditemdetail__cards">
              <div className="bditemdetail__card">
                <div className="bditemdetail__cardhead">
                  <div className="bditemdetail__title u-truncate">{item.name}</div>
                  <div className="bditemdetail__actions">
                    {isSmart ? (
                      <button type="button" className="bditemdetail__editbtn">
                        Edit
                      </button>
                    ) : null}
                    <button type="button" className="bditemdetail__editbtn">
                      Preview in Editor
                    </button>
                    <div className="bditemdetail__ctxwrap">
                      <button
                        type="button"
                        className="bditemdetail__more"
                        aria-label="Item options"
                        aria-expanded={menuOpen}
                        onClick={() => setMenuOpen((v) => !v)}
                      >
                        <DotsGlyph />
                      </button>
                      {menuOpen ? (
                        <ul className="bditemdetail__dropdown" role="menu">
                          <li role="menuitem" className="is-disabled" aria-disabled title="Not available on this realm yet">
                            Delete
                          </li>
                          <li role="menuitem" className="is-disabled" aria-disabled title="Not available on this realm yet">
                            Move to another collection
                          </li>
                        </ul>
                      ) : null}
                    </div>
                  </div>
                </div>

                <div className="bditemdetail__attrrow">
                  {item.description ? (
                    <div className="bditemdetail__attrcol">
                      <div className="bditemdetail__subtitle">Description</div>
                      <div className="bditemdetail__attrdata">{item.description}</div>
                    </div>
                  ) : null}
                  {item.utility ? (
                    <div className="bditemdetail__attrcol">
                      <div className="bditemdetail__subtitle">Utility</div>
                      <div className="bditemdetail__attrdata">{item.utility}</div>
                    </div>
                  ) : null}
                </div>

                {item.tags.length ? (
                  <div className="bditemdetail__attrrow">
                    <div className="bditemdetail__attrcol">
                      <div className="bditemdetail__subtitle">Tags</div>
                      <div className="bditemdetail__tags">
                        {item.tags.map((tag) => (
                          <span className="bditemdetail__tag" key={tag}>
                            {tag}
                          </span>
                        ))}
                      </div>
                    </div>
                  </div>
                ) : null}
              </div>

              <div className="bditemdetail__card">
                <div className="bditemdetail__title">Selling</div>
                <div className="bditemdetail__selling">
                  <div className="bditemdetail__pricebox">
                    <div className="bditemdetail__subtitle">Price</div>
                    {item.price === "free" ? (
                      <div className="bditemdetail__value">Free</div>
                    ) : item.price === "not_for_sale" ? (
                      <div className="bditemdetail__value">Not for sale</div>
                    ) : item.price ? (
                      <div className="bditemdetail__mana">
                        <ManaGlyph />
                        <span className="bditemdetail__value">{item.price}</span>
                      </div>
                    ) : (
                      <div className="bditemdetail__value">-</div>
                    )}
                  </div>
                  <div className="bditemdetail__benbox">
                    <div className="bditemdetail__subtitle">Beneficiary</div>
                    <div className="bditemdetail__value">{item.beneficiary || "-"}</div>
                  </div>
                </div>
              </div>

              {showRepresentations ? (
                <div className="bditemdetail__card">
                  <div className="bditemdetail__cardhead">
                    <div className="bditemdetail__title">Representations</div>
                    <button type="button" className="bditemdetail__editbtn">
                      Edit
                    </button>
                  </div>
                  <div className="bditemdetail__reps">
                    {item.representations.map((rep) => (
                      <div key={rep.mainFile} className="bditemdetail__rep">
                        <span
                          className="bditemdetail__repimg"
                          style={{ background: `linear-gradient(135deg, hsl(${item.hue} 60% 40%), hsl(${(item.hue + 40) % 360} 55% 24%))` }}
                        />
                        <span className="bditemdetail__repname">{rep.mainFile}</span>
                        <BodyBadge bodyShape={rep.bodyShape} />
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}

              {showPermissions ? (
                <div className="bditemdetail__card">
                  <div className="bditemdetail__cardhead">
                    <div className="bditemdetail__title">Permissions</div>
                    <a
                      className="bditemdetail__editbtn"
                      href="https://docs.decentraland.org/creator/development-guide/sdk7/scene-metadata/#required-permissions"
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      Learn more
                    </a>
                  </div>
                  <div className="bditemdetail__perms">
                    {item.requiredPermissions.map((perm) => (
                      <span key={perm} className="bditemdetail__perm">
                        {perm.replaceAll("_", " ").toLowerCase()}
                      </span>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </div>
  );

  return body;
}
