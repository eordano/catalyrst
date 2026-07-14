import type { CSSProperties } from "react";
import "./landingstory.css";

export type LandingStoryBeat = {
  title: string;
  body: string;
  cta?: { label: string; href: string };
};

type LandingStoryProps = {
  audience?: string;
  headline?: string;
  subhead?: string;
  beats?: LandingStoryBeat[];
  cta?: { label: string; href: string };
};

const PROOF = [
  "Keep ~90%",
  "Own & port your work",
  "Build in the browser",
  "Paid in USD Credits",
  "Can't be deplatformed",
];

const ArrowMark = () => (
  <svg className="ls__arrow" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path
      d="M5 12h13M13 6l6 6-6 6"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const delay = (i: number): CSSProperties => ({ ["--d"]: String(i) } as CSSProperties);

export default function LandingStory({
  audience = "Creators & players",
  headline = "Make what you want. Keep what you make.",
  subhead = "Decentraland is the social, creator-owned medium for 3D worlds.",
  beats = [],
  cta = { label: "Get started", href: "/create" },
}: LandingStoryProps) {
  const words = headline.split(" ");
  const kickerD = 0;
  const wordStart = 1;
  const subD = wordStart + words.length;
  const actionsD = subD + 1;
  const tickerD = actionsD + 1;

  return (
    <section className="ls">
      <div className="ls__hero">
        <div className="ls__bg" aria-hidden="true">
          <div className="ls__grid" />
          <div className="ls__aurora ls__aurora--a" />
          <div className="ls__aurora ls__aurora--b" />
          <div className="ls__aurora ls__aurora--c" />
          <div className="ls__grain" />
          <div className="ls__vignette" />
        </div>

        <div className="ls__stage">
          <p className="ls__kicker ls__rise" style={delay(kickerD)}>
            <span className="ls__pip" aria-hidden="true" />
            For <span className="ls__aud">{audience}</span>
          </p>

          <h1 className="ls__headline">
            {words.map((w, i) => (
              <span className="ls__word" key={`${i}-${w}`}>
                <span className="ls__wordin" style={delay(wordStart + i)}>
                  {w}
                </span>
              </span>
            ))}
          </h1>

          <p className="ls__subhead ls__rise" style={delay(subD)}>
            {subhead}
          </p>

          <div className="ls__actions ls__rise" style={delay(actionsD)}>
            <a className="ls__cta" href={cta.href}>
              <span className="ls__ctatext">{cta.label}</span>
              <ArrowMark />
            </a>
            <span className="ls__reassure">Free to start &#xB7; no download</span>
          </div>

          <div
            className="ls__ticker ls__rise"
            style={delay(tickerD)}
            aria-label="What Decentraland guarantees"
          >
            <div className="ls__tickmask">
              <div className="ls__ticktrack">
                {[0, 1].map((dup) =>
                  PROOF.map((p, i) => (
                    <span className="ls__tick" key={`${dup}-${i}`} aria-hidden={dup === 1}>
                      <span className="ls__tickstar" aria-hidden="true">
                        &#x2726;
                      </span>
                      {p}
                    </span>
                  )),
                )}
              </div>
            </div>
          </div>
        </div>

        <span className="ls__scroll" aria-hidden="true">
          <span />
        </span>
      </div>

      {beats.length > 0 ? (
        <ol className="ls__beats">
          {beats.map((b, i) => (
            <li className="ls__beat ls__reveal" style={delay(i)} key={b.title}>
              <span className="ls__beatnum" aria-hidden="true">
                {String(i + 1).padStart(2, "0")}
              </span>
              <div className="ls__beatcopy">
                <h2 className="ls__beattitle">{b.title}</h2>
                <p className="ls__beattext">{b.body}</p>
              </div>
              {b.cta ? (
                <a className="ls__beatcta" href={b.cta.href}>
                  <span>{b.cta.label}</span>
                  <ArrowMark />
                </a>
              ) : null}
              <span className="ls__beatline" aria-hidden="true" />
            </li>
          ))}
        </ol>
      ) : null}
    </section>
  );
}
