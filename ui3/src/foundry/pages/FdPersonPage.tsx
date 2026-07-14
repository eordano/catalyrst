import FdPersonaChip, { type FdAvatarSpec } from "../components/FdPersonaChip";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import { dayUTC, roleLabel, stampUTC } from "../fmt";
import "../components/fdpersonachip.css";
import "./fdperson.css";

// One person, everything the site honestly recorded about them: their own
// words, what they steward, what they asked for and pledged to, and their
// recorded acts. Every line is a DB row; absent sections say so.

export type FdPersonStewardingVM = {
  sceneId: string;
  sceneTitle: string;
  since: string;
  basis: string;
};

export type FdPersonAskVM = { id: string; title: string; at: string };

export type FdPersonActVM = {
  at: string;
  action: string;
  subjectLabel: string | null;
  subjectKind: "scene" | "session" | "request" | "doc" | null;
  subjectId: string | null;
};

export type FdPersonVM = {
  displayName: string;
  avatar: FdAvatarSpec | null;
  words: string | null;
  claimedAt: string;
  roles: readonly string[];
  lastSeen: string | null;
  stewarding: readonly FdPersonStewardingVM[];
  asks: readonly FdPersonAskVM[];
  pledges: readonly FdPersonAskVM[];
  acts: readonly FdPersonActVM[];
};

export type FdPersonPageProps = {
  person: FdPersonVM;
};

/** Stored action names → plain sentences. An action outside the map renders
 *  its stored name in mono rather than an invented verb. */
const ACT_VERBS: Record<string, string> = {
  claim_persona: "claimed this persona",
  update_persona: "updated this persona",
  claim_steward: "claimed stewardship of",
  release_steward: "released stewardship of",
  scene_note: "left a note on",
  offer_transfer: "offered a transfer of",
  revoke_transfer: "revoked a transfer of",
  accept_transfer: "accepted a transfer of",
  schedule_session: "scheduled",
  retire_session: "retired",
  rsvp_session: "RSVPed to",
  withdraw_rsvp: "withdrew an RSVP from",
  post_request: "asked for",
  pledge: "pledged on",
  withdraw_pledge: "withdrew a pledge on",
  approve_request: "approved",
  close_request: "closed",
  approve_gdd: "approved design doc",
  edit_gdd_doc: "edited design doc",
  publish_gdd_draft: "published design doc",
};

function subjectHref(act: FdPersonActVM): string | null {
  if (act.subjectId === null) return null;
  if (act.subjectKind === "scene") return `/foundry/play/${act.subjectId}`;
  if (act.subjectKind === "request")
    return `/foundry/exchange/${act.subjectId}`;
  if (act.subjectKind === "session") return "/foundry/sessions";
  if (act.subjectKind === "doc") return `/foundry/gdd/${act.subjectId}`;
  return null;
}

export default function FdPersonPage({ person }: FdPersonPageProps) {
  return (
    <div className="fd-page fd-stack fd-person">
      <FdPageHead
        title={person.displayName}
        eyebrow="People"
        crumbs={<a href="/foundry/people">← All people</a>}
        aside={
          <FdPersonaChip
            actor={{ name: person.displayName, avatar: person.avatar }}
            showAvatar
          />
        }
      />

      {person.words ? <p className="fd-person__words">{person.words}</p> : null}

      <dl className="fd-facts">
        <div>
          <dt>Claimed</dt>
          <dd>
            <FdTime iso={person.claimedAt} title={stampUTC(person.claimedAt)}>
              {dayUTC(person.claimedAt)}
            </FdTime>
          </dd>
        </div>
        <div>
          <dt>Last seen</dt>
          <dd>
            {person.lastSeen === null ? (
              <span className="fd-note-inline">no recorded action</span>
            ) : (
              <FdTime iso={person.lastSeen} title={stampUTC(person.lastSeen)}>
                {dayUTC(person.lastSeen)}
              </FdTime>
            )}
          </dd>
        </div>
        {person.roles.length > 0 ? (
          <div>
            <dt>Roles</dt>
            <dd className="fd-chiprow">
              {person.roles.map((r) => (
                <span key={r} className="fd-chip">
                  {roleLabel(r)}
                </span>
              ))}
            </dd>
          </div>
        ) : null}
      </dl>

      <FdSection title="Stewarding">
        {person.stewarding.length === 0 ? (
          <p className="fd-empty">
            No claims. Stewardship is claimed on a game&apos;s page, in your own
            words.
          </p>
        ) : (
          <ul className="fd-person__list">
            {person.stewarding.map((s) => (
              <li key={s.sceneId}>
                <a href={`/foundry/play/${s.sceneId}`}>{s.sceneTitle}</a>{" "}
                <span className="fd-note-inline">
                  since {dayUTC(s.since)} — “{s.basis}”
                </span>
              </li>
            ))}
          </ul>
        )}
      </FdSection>

      <FdSection title="Asked and pledged">
        {person.asks.length === 0 && person.pledges.length === 0 ? (
          <p className="fd-empty">
            Nothing yet. Asks and pledges live on{" "}
            <a href="/foundry/exchange">the exchange</a>.
          </p>
        ) : (
          <ul className="fd-person__list">
            {person.asks.map((a) => (
              <li key={`ask-${a.id}`}>
                asked for <a href={`/foundry/exchange/${a.id}`}>{a.title}</a>{" "}
                <FdTime iso={a.at} className="fd-note-inline" title={stampUTC(a.at)}>
                  {dayUTC(a.at)}
                </FdTime>
              </li>
            ))}
            {person.pledges.map((a) => (
              <li key={`pledge-${a.id}`}>
                pledged on <a href={`/foundry/exchange/${a.id}`}>{a.title}</a>{" "}
                <FdTime iso={a.at} className="fd-note-inline" title={stampUTC(a.at)}>
                  {dayUTC(a.at)}
                </FdTime>
              </li>
            ))}
          </ul>
        )}
      </FdSection>

      <FdSection title="Recorded acts">
        {person.acts.length === 0 ? (
          <p className="fd-empty">No recorded acts.</p>
        ) : (
          <ol className="fd-person__acts">
            {person.acts.map((act, i) => {
              const href = subjectHref(act);
              return (
                <li key={i}>
                  <FdTime iso={act.at} className="fd-mono" title={stampUTC(act.at)}>
                    {dayUTC(act.at)}
                  </FdTime>{" "}
                  {ACT_VERBS[act.action] ?? (
                    <span className="fd-mono">{act.action}</span>
                  )}{" "}
                  {act.subjectLabel !== null ? (
                    href ? (
                      <a href={href}>{act.subjectLabel}</a>
                    ) : (
                      act.subjectLabel
                    )
                  ) : null}
                </li>
              );
            })}
          </ol>
        )}
      </FdSection>
    </div>
  );
}
