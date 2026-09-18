import type { ReactNode } from "react";

import Button from "../../atoms/Button";
import { Close } from "../../atoms/icons";
import type { Option, ReportCard } from "./AdReportTypes";
import { placeArt, reasonLabel, statusLabel, statusTone } from "./AdReportTypes";

type ReportReviewPanelProps = {
  card: ReportCard;
  reasons: Option[];
  onClose: () => void;
};

function Fact({ label, mono = false, children }: { label: string; mono?: boolean; children: ReactNode }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd className={mono ? "adm-mono" : undefined}>{children}</dd>
    </div>
  );
}

export default function AdReportReviewPanel({
  card,
  reasons,
  onClose,
}: ReportReviewPanelProps) {
  const entityUrl = card.placeCoords
    ? `https://decentraland.org/play/?position=${encodeURIComponent(card.placeCoords)}`
    : undefined;

  return (
    <div className="adm-stack" role="region" aria-label={`Review report ${card.id}`}>
      <div className="adm-card__head">
        <h2 className="adm-card__title">Report #{card.id}</h2>
        <span className="adm-status" data-tone={statusTone(card.status)}>
          {statusLabel(card.status)}
        </span>
        <Button variant="ghost" size="sm" onClick={onClose} aria-label="Back to queue">
          <Close size={16} />
        </Button>
      </div>

      <div className="adm-media">
        <div className="adm-thumb" style={placeArt(card)} role="img" aria-label={card.placeTitle} />

        <dl className="adm-stats adm-stats--grid">
          <Fact label="Reported place">
            {entityUrl ? (
              <a className="adm-link" href={entityUrl} target="_blank" rel="noreferrer">
                {card.placeTitle}
              </a>
            ) : (
              card.placeTitle
            )}
          </Fact>
          <Fact label="Coordinates">{card.placeCoords ?? "\u{2014}"}</Fact>
          <Fact label="Entity id" mono>{card.entityId ?? "\u{2014}"}</Fact>
          <Fact label="Place creator">{card.placeCreator ?? "\u{2014}"}</Fact>
          <Fact label="Reason">{reasonLabel(reasons, card.reason)}</Fact>
          <Fact label="Reporter" mono>{card.reporter}</Fact>
          <Fact label="Reported at">{card.createdLabel}</Fact>
        </dl>
      </div>

      {card.notes && (
        <p className="adm-card__text">
          <strong>Report notes:</strong> {card.notes}
        </p>
      )}
      {card.status !== "open" && card.resolution && (
        <p className="adm-card__text adm-dim">
          <strong>Prior resolution:</strong> {card.resolution}
          {card.resolvedBy ? ` (by ${card.resolvedBy})` : ""}
        </p>
      )}
    </div>
  );
}
