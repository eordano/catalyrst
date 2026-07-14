import type { CSSProperties, ReactNode } from "react";
import "./upcomingeventcard.css";

type UpcomingEvent = {
  title: ReactNode;
  when?: ReactNode;
  hue?: number;
  glyph?: ReactNode;
  started?: ReactNode;
  badge?: string;
  live?: boolean | string;
  image?: string;
};

type UpcomingEventCardProps = { event: UpcomingEvent };

export default function UpcomingEventCard({ event }: UpcomingEventCardProps) {
  const { title, when, hue = 280, glyph, started, badge, live, image } = event;
  const label = badge ?? (typeof live === "string" ? live : live ? "LIVE" : null);
  return (
    <article className="uec" style={{ "--hue": hue } as CSSProperties}>
      <div
        className="uec__thumb"
        style={image ? ({ "--thumb-img": `url("${image}")` } as CSSProperties) : undefined}
      >
        {glyph ? <span className="uec__glyph" aria-hidden="true">{glyph}</span> : null}
        {label ? <span className="uec__live">{label}</span> : null}
        {started ? <span className="uec__started u-truncate">{started}</span> : null}
      </div>
      <div className="uec__body">
        {when ? <div className="uec__when">{when}</div> : null}
        <div className="uec__title">{title}</div>
      </div>
    </article>
  );
}
