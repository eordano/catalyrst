import { plural, roleLabel, stampUTC } from "../fmt";
import Button from "../../atoms/Button";
import FdAvatarCrowd, {
  type FdAvatarCrowdViewer,
} from "../components/FdAvatarCrowd";
import FdSection, { FD_UNAVAILABLE, FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import "./fdselect.css";

/** The soonest occurrence on the community calendar, read from the sessions DB. */
export type FdSelectNextSession = {
  title: string;
  occurrenceAt: string;
  rsvpCount: number;
};

export type FdSelectPageProps = {
  viewer?: FdAvatarCrowdViewer | null;
  /** Stored role names this browser session actively holds. */
  heldRoles?: readonly string[];
  /** undefined = calendar unread (no database); null = calendar read and empty;
   *  an object = the next session on it. */
  nextSession?: FdSelectNextSession | null;
};

export default function FdSelectPage({
  viewer = null,
  heldRoles = [],
  nextSession,
}: FdSelectPageProps) {
  return (
    <div className="fd-page fd-stack fd-select">
      <FdPageHead
        title="Your people"
        aside={
          <>
            {heldRoles.map((role) => (
              <span
                key={role}
                className="fd-chip"
                title="granted to this browser session"
              >
                {roleLabel(role)}
              </span>
            ))}
            <Button as="a" variant="secondary" size="sm" href="/foundry">
              The three doors
            </Button>
          </>
        }
      />

      <FdSection title="Avatars">
        <div className="fd-select__crowd">
          <FdAvatarCrowd
            viewer={viewer}
            caption={
              viewer
                ? "Your avatar, next to Decentraland's base avatars."
                : "Decentraland's base avatars. Sign in and the first one becomes yours."
            }
          />
        </div>
      </FdSection>

      <FdSection title="Next session">
        {nextSession === undefined ? (
          <p className="fd-empty">{FD_UNAVAILABLE}</p>
        ) : nextSession ? (
          <div className="fd-board">
            <div className="fd-card">
              <h3 className="fd-card__title">
                <a className="fd-cardlink" href="/foundry/sessions">
                  {nextSession.title}
                </a>
              </h3>
              <div className="fd-chiprow">
                <FdTime
                  iso={nextSession.occurrenceAt}
                  className="fd-chip fd-chip--mono"
                  title="the soonest occurrence on the calendar"
                >
                  {stampUTC(nextSession.occurrenceAt)}
                </FdTime>
                <span className="fd-chip" title="RSVPs recorded on this session">
                  {plural(nextSession.rsvpCount, "RSVP")}
                </span>
              </div>
            </div>
          </div>
        ) : (
          <p className="fd-empty">No session on the calendar yet.</p>
        )}
      </FdSection>
    </div>
  );
}
