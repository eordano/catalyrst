import Button from "../../atoms/Button";
import type {
  Option,
  QueueBuckets,
  ReportCard,
  ReportStatus,
} from "./AdReportTypes";
import { reasonLabel, statusLabel } from "./AdReportTypes";

export type ReportQueueProps = {
  buckets: QueueBuckets;
  reasons: Option[];
  total: number;
  onOpen: (reportId: string) => void;
  activeId?: string;
};

const BUCKET_ORDER: { key: keyof QueueBuckets; status: ReportStatus }[] = [
  { key: "open", status: "open" },
  { key: "actioned", status: "actioned" },
  { key: "resolved", status: "resolved" },
  { key: "dismissed", status: "dismissed" },
];

export default function AdReportQueue({
  buckets,
  reasons,
  total,
  onOpen,
  activeId,
}: ReportQueueProps) {
  return (
    <div className="rq">
      <div className="rq__head">
        <h2 className="rq__title">Report queue</h2>
        <span className="rq__count">
          {buckets.open.length} open &#xB7; {total} total
        </span>
      </div>

      {BUCKET_ORDER.map(({ key, status }) => {
        const cards = buckets[key];
        return (
          <section key={key} className="rq__bucket" aria-label={`${statusLabel(status)} reports`}>
            <h3 className="rq__buckettitle">
              {statusLabel(status)}
              <span className="rq__bucketcount">{cards.length}</span>
            </h3>
            {cards.length === 0 ? (
              <p className="rq__empty">No {status} reports.</p>
            ) : (
              <ul className="rq__grid">
                {cards.map((card) => (
                  <li key={card.id}>
                    <ReportTile
                      card={card}
                      reasons={reasons}
                      active={card.id === activeId}
                      onOpen={() => onOpen(card.id)}
                    />
                  </li>
                ))}
              </ul>
            )}
          </section>
        );
      })}
    </div>
  );
}

function ReportTile({
  card,
  reasons,
  active,
  onOpen,
}: {
  card: ReportCard;
  reasons: Option[];
  active: boolean;
  onOpen: () => void;
}) {
  return (
    <article className={"rq-card" + (active ? " is-active" : "")}>
      <div
        className="rq-card__thumb"
        style={
          card.placeImage
            ? { backgroundImage: `url(${card.placeImage})` }
            : { background: `hsl(${card.hue} 60% 38%)` }
        }
        role="img"
        aria-label={card.placeTitle}
      >
        <span className={`aq-chip aq-chip--status rq-card__chip rq-card__chip--${card.status}`}>
          {statusLabel(card.status)}
        </span>
      </div>
      <div className="rq-card__body">
        <h4 className="rq-card__title">{card.placeTitle}</h4>
        <p className="rq-card__meta">
          {card.placeCoords ?? "\u{2014}"} &#xB7; {reasonLabel(reasons, card.reason)}
        </p>
        <p className="rq-card__sub">
          #{card.id} &#xB7; by {card.reporterShort} &#xB7; {card.createdLabel}
        </p>
        <Button variant="secondary" size="sm" className="rq-card__btn" onClick={onOpen}>
          Review
        </Button>
      </div>
    </article>
  );
}
