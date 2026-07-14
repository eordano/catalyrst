import type { CSSProperties } from "react";

type PlaceCardProps = {
  title: string;
  image?: string;
  players: number | null;
  rating: number;
  coords?: string;
  live?: number;
  featured?: boolean;
  creator: string;
  hue?: number;
  to?: string;
  // Render as a non-interactive <article> (no role="button"/tabIndex) even when
  // titled, for callers that wrap the card in their own <a>/<Link> -- avoids an
  // interactive control nested inside an interactive control (double tab stop,
  // invalid ARIA). The wrapping element supplies the accessible name + click.
  presentational?: boolean;
  skeleton?: false;
};

type PlaceCardSkeletonProps = { skeleton: true } & Partial<Omit<PlaceCardProps, "skeleton">>;

type CardBodyProps = {
  title: string;
  image?: string;
  players: number | null;
  rating: number;
  coords?: string;
  live?: number;
  featured?: boolean;
  creator: string;
  hue: number;
};

function CardBody({ title, image, players, rating, coords, live, featured, creator, hue }: CardBodyProps) {
  const thumbStyle: CSSProperties & { "--hue": number; "--thumb-img"?: string } = {
    "--hue": hue,
    ...(image ? { "--thumb-img": `url("${image}")` } : null),
  };
  return (
    <>
      <div
        className="pl__thumb"
        style={thumbStyle}
        aria-hidden="true"
      />

      <div className="pl__badges">
        {live != null && (
          <span className="pl__badge pl__badge--live">
            <svg viewBox="0 0 24 24" width="11" height="11" fill="none"
              stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
              <path d="M5.6 8.6a8 8 0 0 0 0 6.8M8.5 10.5a4 4 0 0 0 0 3M18.4 8.6a8 8 0 0 1 0 6.8M15.5 10.5a4 4 0 0 1 0 3" />
              <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
            </svg>
            LIVE
          </span>
        )}
        <span
          className="pl__badge pl__badge--players"
          title={players == null ? "Live player count unavailable" : undefined}
        >
          <span className="pl__onlinedot" />
          <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true">
            <path d="M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm0 10c-4.4 0-8 2.2-8 5v1h16v-1c0-2.8-3.6-5-8-5Z" />
          </svg>
          {players ?? "\u{2014}"}
        </span>
      </div>

      {featured && (
        <span className="pl__featured">
          <svg viewBox="0 0 24 24" width="11" height="11" fill="none"
            stroke="currentColor" strokeWidth="2" strokeLinecap="round"
            strokeLinejoin="round" aria-hidden="true">
            <circle cx="12" cy="9" r="5" />
            <path d="M9 13.4 7.5 21l4.5-2.6L16.5 21 15 13.4" />
          </svg>
          Featured
        </span>
      )}

      {title && (
        <div className="pl__info">
          <div className="pl__infotext">
            <span className="pl__cardtitle u-truncate">{title}</span>
            <span className="pl__cardsub u-truncate">{creator}</span>
          </div>
          <div className="pl__infometa">
            <span className="pl__rating">
              <svg viewBox="0 0 24 24" width="13" height="13" fill="none"
                stroke="currentColor" strokeWidth="2" strokeLinejoin="round" aria-hidden="true">
                <path d="M7 22V11M7 13H4v9h3M11 11V6a2 2 0 0 0-4 0v5M11 11l3-5a1.6 1.6 0 0 1 2.8 1.4L16 11h2.6a2 2 0 0 1 2 2.4l-1.2 6a2 2 0 0 1-2 1.6H7" />
              </svg>
              {rating}%
            </span>
            {coords && (
              <span className="pl__coords u-truncate">
                <svg viewBox="0 0 24 24" width="11" height="11" fill="none"
                  stroke="currentColor" strokeWidth="2" aria-hidden="true">
                  <path d="M12 21s7-6.2 7-11a7 7 0 1 0-14 0c0 4.8 7 11 7 11Z" />
                  <circle cx="12" cy="10" r="2.4" />
                </svg>
                {coords}
              </span>
            )}
          </div>
        </div>
      )}
    </>
  );
}

export default function PlaceCard({
  title = "",
  image,
  players = null,
  rating = 0,
  coords,
  live,
  featured,
  creator = "",
  hue = 0,
  to,
  presentational,
  skeleton,
}: PlaceCardProps | PlaceCardSkeletonProps) {
  if (skeleton) {
    return (
      <article className="pl__card pl__card--skeleton" aria-hidden="true">
        <div className="pl__thumb pl__thumb--skeleton" />
        <div className="pl__info pl__info--skeleton">
          <div className="pl__infotext">
            <span className="pl__skelbar pl__skelbar--title" />
            <span className="pl__skelbar pl__skelbar--sub" />
          </div>
          <span className="pl__skelbar pl__skelbar--meta" />
        </div>
      </article>
    );
  }
  const body = (
    <CardBody
      title={title} image={image} players={players} rating={rating}
      coords={coords} live={live} featured={featured} creator={creator} hue={hue}
    />
  );
  if (title && !presentational) {
    return (
      <div
        className="pl__card"
        data-sb-linkto={to || undefined}
        role="button"
        tabIndex={0}
        aria-label={title}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            e.currentTarget.click();
          }
        }}
      >
        {body}
      </div>
    );
  }
  return (
    <article className="pl__card" data-sb-linkto={to || undefined}>
      {body}
    </article>
  );
}
