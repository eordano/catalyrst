import type { CSSProperties } from "react";
import { useState } from "react";
import { Avatar } from "../../atoms/primitives";
import Button from "../../atoms/Button";
import { setPlaceFavorite, setPlaceLike } from "../../data/catalyst/places";
import "./placedetail.css";

type PlaceDetailData = {
  id?: string;
  title: string;
  coords: string;
  parcels: number;
  favorites: number;
  views: number;
  approval: number;
  creator: string;
  updated: string;
  description: string;
  hue: number;
  image?: string | null;
  world?: boolean;
  worldName?: string | null;
};

const PLACE: PlaceDetailData = {
  title: "Genesis Plaza",
  coords: "-3, -2",
  parcels: 70,
  favorites: 129,
  views: 0,
  approval: 100,
  creator: "Decentraland Foundation",
  updated: "16/06/2026",
  description:
    "Decentraland's official spawn point and your #1 place to meet new " +
    "people, hang out, and stay up to date with what's happening in our " +
    "virtual world.",
  hue: 270,
};

type PlaceDetailProps = {
  place?: PlaceDetailData;
  notFound?: boolean;
  onClose?: () => void;
  onJumpIn?: () => void;
};

export default function PlaceDetail({ place, notFound = false, onClose, onJumpIn }: PlaceDetailProps = {}) {
  if (notFound) {
    return (
      <div className="ep__backdrop" onClick={onClose}>
        <div
          className="pld"
          onClick={(e) => e.stopPropagation()}
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            textAlign: "center",
            gap: 8,
            padding: "56px 32px",
          }}
        >
          <h2 style={{ margin: 0 }}>Place not found</h2>
          <p style={{ margin: 0, color: "var(--ink-7, #a09ba8)" }}>
            This place doesn&apos;t exist, or its data is no longer in the catalog.
          </p>
        </div>
      </div>
    );
  }
  const p = place ?? PLACE;
  const [liked, setLiked] = useState<boolean | null>(null);
  const [faved, setFaved] = useState(false);
  const like = (v: boolean) => {
    const next = liked === v ? null : v;
    setLiked(next);
    if (p.id) void setPlaceLike(p.id, next);
  };
  const favorite = () => {
    const next = !faved;
    setFaved(next);
    if (p.id) void setPlaceFavorite(p.id, next);
  };
  const heroStyle: CSSProperties & { "--hue": number; "--thumb-img"?: string } = {
    "--hue": p.hue,
    ...(p.image ? { "--thumb-img": `url("${p.image}")` } : null),
  };
  return (
    <div className="ep__backdrop" onClick={onClose}>
      <div className="pld" onClick={(e) => e.stopPropagation()}>
        <button className="pld__close" aria-label="Close" data-sb-linkto="Explorer/Pages/Places" onClick={onClose}>×</button>

        <div className="pld__top">
          <div
            className="pld__hero"
            style={heroStyle}
            aria-hidden="true"
          />

          <div className="pld__body">
            <header className="pld__head">
            <h2 className="pld__title">{p.title}</h2>
            <div className="pld__creator">
              <Avatar hue={p.hue} size={20} />
              <span className="pld__creatorby">Created by</span>
              <span className="pld__creatorname">{p.creator}</span>
            </div>
          </header>

          <div className="pld__toolbar">
            <div className="pld__approval">
              <span className="pld__stat-inline">
                <svg viewBox="0 0 18 18" width="15" height="15" aria-hidden="true">
                  <path d="M1 9s3-5 8-5 8 5 8 5-3 5-8 5-8-5-8-5z" fill="none" stroke="currentColor" strokeWidth="1.4" />
                  <circle cx="9" cy="9" r="2.2" fill="currentColor" />
                </svg>
                {p.views}
              </span>
              <span className="pld__stat-inline">
                <svg viewBox="0 0 18 18" width="15" height="15" aria-hidden="true">
                  <path d="M5 8.5V15H3V8.5h2zm2 0L8.7 3.6c.2-.6.8-1 1.4-.9.7.1 1.2.8 1.1 1.5L10.7 7H14a1.4 1.4 0 011.4 1.7l-1 5A1.6 1.6 0 0112.8 15H7V8.5z"
                    fill="currentColor" />
                </svg>
                {p.approval}%
              </span>
            </div>
            <div className="pld__icons">
              <button className={"pld__iconbtn" + (liked === true ? " is-active" : "")} aria-label="Like" aria-pressed={liked === true} onClick={() => like(true)}>
                <svg viewBox="0 0 18 18" width="16" height="16" aria-hidden="true">
                  <path d="M5 8.5V15H3V8.5h2zm2 0L8.7 3.6c.2-.6.8-1 1.4-.9.7.1 1.2.8 1.1 1.5L10.7 7H14a1.4 1.4 0 011.4 1.7l-1 5A1.6 1.6 0 0112.8 15H7V8.5z"
                    fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
                </svg>
              </button>
              <button className={"pld__iconbtn" + (liked === false ? " is-active" : "")} aria-label="Dislike" aria-pressed={liked === false} onClick={() => like(false)}>
                <svg viewBox="0 0 18 18" width="16" height="16" aria-hidden="true">
                  <g transform="rotate(180 9 9)">
                    <path d="M5 8.5V15H3V8.5h2zm2 0L8.7 3.6c.2-.6.8-1 1.4-.9.7.1 1.2.8 1.1 1.5L10.7 7H14a1.4 1.4 0 011.4 1.7l-1 5A1.6 1.6 0 0112.8 15H7V8.5z"
                      fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
                  </g>
                </svg>
              </button>
              <button className={"pld__iconbtn" + (faved ? " is-active" : "")} aria-label="Favorite" aria-pressed={faved} onClick={favorite}>
                <svg viewBox="0 0 18 18" width="16" height="16" aria-hidden="true">
                  <path d="M9 15.5S2 11 2 6.5A3.5 3.5 0 019 4a3.5 3.5 0 017 2.5C16 11 9 15.5 9 15.5z"
                    fill="none" stroke="currentColor" strokeWidth="1.4" />
                </svg>
              </button>
              <button className="pld__iconbtn" aria-label="Home">
                <svg viewBox="0 0 18 18" width="16" height="16" aria-hidden="true">
                  <path d="M3 8.5L9 3l6 5.5V15a1 1 0 01-1 1h-3v-4H7v4H4a1 1 0 01-1-1V8.5z"
                    fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
                </svg>
              </button>
              <button
                className="pld__iconbtn"
                aria-label="Share"
                onClick={() => {
                  try {
                    navigator.clipboard?.writeText(`${p.title} — jump to ${p.coords} in Decentraland`);
                  } catch {
                  }
                }}
              >
                <svg viewBox="0 0 18 18" width="16" height="16" aria-hidden="true">
                  <circle cx="5" cy="9" r="2" fill="none" stroke="currentColor" strokeWidth="1.4" />
                  <circle cx="13" cy="4" r="2" fill="none" stroke="currentColor" strokeWidth="1.4" />
                  <circle cx="13" cy="14" r="2" fill="none" stroke="currentColor" strokeWidth="1.4" />
                  <path d="M6.8 8L11.2 5M6.8 10l4.4 3" stroke="currentColor" strokeWidth="1.4" />
                </svg>
              </button>
            </div>
          </div>

          <div className="pld__actions">
            <Button variant="primary" className="pld__nav" data-sb-linkto="Explorer/Workflows/SceneLoading" onClick={onJumpIn}>
              <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                <path d="M8 1.5l6.5 13L8 11.5 1.5 14.5 8 1.5z" fill="currentColor" />
              </svg>
              START NAVIGATION
            </Button>
            <Button variant="primary" className="pld__jump" data-sb-linkto="Explorer/Workflows/SceneLoading" onClick={onJumpIn}>
              JUMP IN
              <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                <path d="M3 8h8M8 4l4 4-4 4" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </Button>
          </div>
          </div>
        </div>

        <div className="pld__detail">
            <section className="pld__section">
              <h3 className="pld__sectitle">DESCRIPTION</h3>
              <p className="pld__desc">{p.description}</p>
            </section>

            <div className="pld__rows">
              <div className="pld__row">
                <div className="pld__rowcell">
                  <div className="pld__rowlabel">
                    {p.world ? (
                      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                        <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.3" />
                        <path d="M1.5 8h13M8 1.5c1.7 1.7 1.7 10.3 0 13M8 1.5c-1.7 1.7-1.7 10.3 0 13"
                          fill="none" stroke="currentColor" strokeWidth="1.1" />
                      </svg>
                    ) : (
                      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                        <path d="M8 1.5c-2.5 0-4.5 2-4.5 4.5C3.5 9.5 8 14.5 8 14.5s4.5-5 4.5-8.5C12.5 3.5 10.5 1.5 8 1.5z"
                          fill="none" stroke="currentColor" strokeWidth="1.3" />
                        <circle cx="8" cy="6" r="1.6" fill="currentColor" />
                      </svg>
                    )}
                    {p.world ? "REALM" : "LOCATION"}
                  </div>
                  <div className="pld__rowval">{p.world ? p.worldName || p.coords : p.coords}</div>
                </div>
                <div className="pld__rowcell">
                  <div className="pld__rowlabel">
                    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                      <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.3" />
                      <path d="M2.5 8h11M8 2.5v11" stroke="currentColor" strokeWidth="1.1" />
                    </svg>
                    PARCELS
                  </div>
                  <div className="pld__rowval">{p.parcels}</div>
                </div>
              </div>

              <div className="pld__row pld__row--bottom">
                <div className="pld__rowcell">
                  <div className="pld__rowlabel pld__rowlabel--plain">FAVORITES</div>
                  <div className="pld__rowval">{p.favorites.toLocaleString()}</div>
                </div>
                <div className="pld__rowcell">
                  <div className="pld__rowlabel pld__rowlabel--plain">UPDATED</div>
                  <div className="pld__rowval">{p.updated}</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
  );
}
