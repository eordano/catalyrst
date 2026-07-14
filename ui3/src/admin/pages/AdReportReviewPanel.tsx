import { Close } from "../../atoms/icons";
import type { Option, ReportCard } from "./AdReportTypes";
import { reasonLabel, statusLabel } from "./AdReportTypes";

export type ReportReviewPanelProps = {
  card: ReportCard;
  reasons: Option[];
  onClose: () => void;
};

export default function AdReportReviewPanel({
  card,
  reasons,
  onClose,
}: ReportReviewPanelProps) {
  const entityUrl = card.placeCoords
    ? `https://decentraland.org/play/?position=${encodeURIComponent(card.placeCoords)}`
    : undefined;

  return (
    <div className="rrp" role="region" aria-label={`Review report ${card.id}`}>
      <div className="rrp__head">
        <h2 className="rrp__title">Report #{card.id}</h2>
        <span className={`aq-chip aq-chip--status rrp__chip rrp__chip--${card.status}`}>
          {statusLabel(card.status)}
        </span>
        <button type="button" className="rrp__close" onClick={onClose} aria-label="Back to queue">
          <Close size={16} />
        </button>
      </div>

      <div className="rrp__grid">
        <div
          className="rrp__thumb"
          style={
            card.placeImage
              ? { backgroundImage: `url(${card.placeImage})` }
              : { background: `hsl(${card.hue} 60% 38%)` }
          }
          role="img"
          aria-label={card.placeTitle}
        />

        <dl className="rrp__facts">
          <div className="rrp__fact">
            <dt>Reported place</dt>
            <dd>
              {entityUrl ? (
                <a href={entityUrl} target="_blank" rel="noreferrer">
                  {card.placeTitle}
                </a>
              ) : (
                card.placeTitle
              )}
            </dd>
          </div>
          <div className="rrp__fact">
            <dt>Coordinates</dt>
            <dd>{card.placeCoords ?? "\u{2014}"}</dd>
          </div>
          <div className="rrp__fact">
            <dt>Entity id</dt>
            <dd className="rrp__mono">{card.entityId ?? "\u{2014}"}</dd>
          </div>
          <div className="rrp__fact">
            <dt>Place creator</dt>
            <dd>{card.placeCreator ?? "\u{2014}"}</dd>
          </div>
          <div className="rrp__fact">
            <dt>Reason</dt>
            <dd>{reasonLabel(reasons, card.reason)}</dd>
          </div>
          <div className="rrp__fact">
            <dt>Reporter</dt>
            <dd className="rrp__mono">{card.reporter}</dd>
          </div>
          <div className="rrp__fact">
            <dt>Reported at</dt>
            <dd>{card.createdLabel}</dd>
          </div>
        </dl>
      </div>

      {card.notes && (
        <p className="rrp__notes">
          <strong>Report notes:</strong> {card.notes}
        </p>
      )}
      {card.status !== "open" && card.resolution && (
        <p className="rrp__notes rrp__notes--prior">
          <strong>Prior resolution:</strong> {card.resolution}
          {card.resolvedBy ? ` (by ${card.resolvedBy})` : ""}
        </p>
      )}
    </div>
  );
}
