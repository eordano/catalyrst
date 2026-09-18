import Button from "../../atoms/Button";
import EmptyState from "../../components/EmptyState";
import type {
  Option,
  QueueBuckets,
  ReportCard,
  ReportStatus,
} from "./AdReportTypes";
import { placeArt, reasonLabel, statusLabel, statusTone } from "./AdReportTypes";

type ReportQueueProps = {
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
  const empty = BUCKET_ORDER.every(({ key }) => buckets[key].length === 0);
  return (
    <div className="adm-stack adm-stack--lg">
      <div className="adm__head">
        <h2 className="adm__title">Report queue</h2>
        <span className="adm__sub">
          {buckets.open.length} open &#xB7; {total} total
        </span>
      </div>

      {empty ? (
        <EmptyState
          variant="inline"
          title="No reports"
          subtitle="Reports filed against places land here for review."
        />
      ) : (
        BUCKET_ORDER.map(({ key, status }) => {
          const cards = buckets[key];
          return (
            <section key={key} className="adm-stack" aria-label={`${statusLabel(status)} reports`}>
              <h3 className="adm__h3">
                {statusLabel(status)} <span className="adm-status">{cards.length}</span>
              </h3>
              {cards.length === 0 ? (
                <p className="adm-card__text adm-dim">No {status} reports.</p>
              ) : (
                <div className="adm-grid">
                  {cards.map((card) => (
                    <ReportTile
                      key={card.id}
                      card={card}
                      reasons={reasons}
                      active={card.id === activeId}
                      onOpen={() => onOpen(card.id)}
                    />
                  ))}
                </div>
              )}
            </section>
          );
        })
      )}
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
    <article className={"adm-card" + (active ? " is-active" : "")}>
      <div className="adm-thumb" style={placeArt(card)} role="img" aria-label={card.placeTitle} />
      <div className="adm-card__head">
        <h4 className="adm-card__title">{card.placeTitle}</h4>
        <span className="adm-status" data-tone={statusTone(card.status)}>
          {statusLabel(card.status)}
        </span>
      </div>
      <p className="adm-card__text">
        {card.placeCoords ?? "\u{2014}"} &#xB7; {reasonLabel(reasons, card.reason)}
      </p>
      <p className="adm-card__text adm-dim">
        #{card.id} &#xB7; by {card.reporterShort} &#xB7; {card.createdLabel}
      </p>
      <div className="adm-actions adm-actions--start">
        <Button variant="secondary" size="sm" onClick={onOpen}>
          Review
        </Button>
      </div>
    </article>
  );
}
