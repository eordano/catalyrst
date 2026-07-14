import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdPersonaChip, { type FdChipActor } from "../components/FdPersonaChip";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import { dayUTC } from "../fmt";
import "../components/fdpersonachip.css";
import "./fdsuccession.css";

export type FdSuccessionOffer = {
  sceneTitle: string;
  from: { badge: string } | { name: string };
  note: string;
  expiresAt: string;
};

export type FdSuccessionDead = "unknown" | "expired" | "revoked" | "accepted";

export type FdSuccessionView =
  | { ok: true; offer: FdSuccessionOffer }
  | { ok: false; reason: FdSuccessionDead };

export type FdSuccessionPageProps = {
  view: FdSuccessionView;
  pending?: boolean;
  error?: string | null;
  onAccept: () => void;
};

const DEAD_COPY: Record<FdSuccessionDead, string> = {
  unknown: "This transfer link is not valid.",
  expired: "This transfer offer has expired.",
  revoked: "This transfer offer was revoked.",
  accepted: "This transfer was already accepted.",
};

export default function FdSuccessionPage({
  view,
  pending = false,
  error = null,
  onAccept,
}: FdSuccessionPageProps) {
  if (!view.ok) {
    return (
      <div className="fd-page fd-stack fd-succession">
        <FdPageHead eyebrow="Stewardship" title="Stewardship transfer" />
        <p className="fd-empty">{DEAD_COPY[view.reason]}</p>
      </div>
    );
  }

  const { offer } = view;
  return (
    <div className="fd-page fd-stack fd-succession">
      <FdPageHead
        eyebrow="Stewardship"
        title={`Take over ${offer.sceneTitle}`}
        intro="A recorded claim, not verified ownership — no deploy keys change hands."
      />

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      <FdSection title="The offer">
        <div className="fd-panel fd-succession__offer">
          <p className="fd-succession__from">
            from <FdPersonaChip actor={offer.from as FdChipActor} />
          </p>
          {offer.note ? <p className="fd-succession__note">{offer.note}</p> : null}
          <p className="fd-note">
            Expires <FdTime iso={offer.expiresAt}>{dayUTC(offer.expiresAt)}</FdTime>.
          </p>
          <div className="fd-succession__actions">
            <Button variant="primary" size="sm" onClick={onAccept} disabled={pending}>
              Accept stewardship
            </Button>
            {pending ? <Spinner size={16} /> : null}
          </div>
        </div>
      </FdSection>
    </div>
  );
}
