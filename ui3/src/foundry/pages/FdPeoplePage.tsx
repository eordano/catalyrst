import { type FormEvent, useEffect, useRef, useState } from "react";

import { dayUTC, plural, roleLabel } from "../fmt";
import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdPersonaChip, { type FdChipActor } from "../components/FdPersonaChip";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import "../components/fdpersonachip.css";
import "./fdpeople.css";

export type FdRoomPresenceVM = { path: string; count: number };

export type FdPersonRow = {
  actor: FdChipActor;
  roles: readonly string[];
  /** Their own self-description, verbatim; null = never written. */
  words?: string | null;
  claimedAt: string;
  requests: number;
  /** Ids of the asks behind the count — one id links straight to that ask. */
  requestIds: readonly string[];
  pledges: number;
  /** Ask ids the session pledged on, in pledge order. */
  pledgeRequestIds: readonly string[];
  lastSeen: string | null;
};

/** A count that reaches its own rows: one ask links to that ask, several link
 *  to the community lane that lists each of them. A count with no destination
 *  renders as a plain chip, so a link is never mimicked by an inert one. */
function FdCountChip({
  count,
  ids,
  noun,
}: {
  count: number;
  ids: readonly string[];
  noun: "request" | "pledge";
}) {
  if (count === 0) return null;
  const [only] = ids;
  const label = plural(count, noun);
  if (ids.length === 1 && only) {
    return (
      <a className="fd-chip" href={`/foundry/exchange/${only}`}>
        {label}
      </a>
    );
  }
  if (ids.length > 1) {
    return (
      <a className="fd-chip" href="/foundry/timeline?lane=community">
        {label}
      </a>
    );
  }
  return <span className="fd-chip">{label}</span>;
}

export type FdRosterEntry = {
  role: string;
  actor: { name: string } | { badge: string };
  since: string;
};

/** A just-minted one-time invite. It exists only in the action response that
 *  delivered it — the ledger records the mint, never the code. */
export type FdMintedInviteVM = { code: string; role: string };

/** The roles a host may hand out from the web. Operator invites only come from
 *  the operator bootstrap script, so "admin" is deliberately not offered. */
const MINTABLE_ROLES = ["start", "create", "host"] as const;

export type FdPeoplePageProps = {
  people: readonly FdPersonRow[];
  roster: { rows: readonly FdRosterEntry[]; notListed: number };
  /** Live SFU reading of the page rooms; null = server unconfigured or the
   *  probe failed — the section then says it could not read, never a zero. */
  presence?: readonly FdRoomPresenceVM[] | null;
  myRoles: readonly string[];
  error?: string | null;
  /** True after a successful redeem — the page confirms and clears the code. */
  redeemed?: boolean;
  /** The redeeming session's persona name, when it has one — the confirmation
   *  then names who holds the role instead of "this session". */
  redeemedName?: string | null;
  pending?: boolean;
  onRedeem: (values: { code: string; consentSteward: boolean }) => void;
  /** Set after a successful mint — rendered once, in the mint section. */
  minted?: FdMintedInviteVM | null;
  mintError?: string | null;
  mintPending?: boolean;
  onMint?: (values: { role: string; note: string; expires: string }) => void;
};

export default function FdPeoplePage({
  people,
  roster,
  myRoles,
  presence = null,
  error = null,
  redeemed = false,
  redeemedName = null,
  pending = false,
  onRedeem,
  minted = null,
  mintError = null,
  mintPending = false,
  onMint,
}: FdPeoplePageProps) {
  const [consent, setConsent] = useState(false);
  const codeRef = useRef<HTMLInputElement>(null);
  const canMint = myRoles.includes("host");

  useEffect(() => {
    if (redeemed && codeRef.current) codeRef.current.value = "";
  }, [redeemed]);

  function submitRedeem(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    onRedeem({
      code: String(form.get("code") ?? "").trim(),
      consentSteward: consent,
    });
  }

  function submitMint(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!onMint) return;
    const form = new FormData(event.currentTarget);
    onMint({
      role: String(form.get("role") ?? ""),
      note: String(form.get("note") ?? ""),
      expires: String(form.get("expires") ?? ""),
    });
  }

  return (
    <div className="fd-page fd-stack fd-people">
      <FdPageHead
        title="People"
        aside={
          <Button as="a" variant="primary" size="sm" href="/foundry/persona">
            Claim a persona
          </Button>
        }
      />

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      <FdSection
        title="Redeem an invite code"
        badge={
          myRoles.length > 0 ? (
            <span className="fd-chip">{myRoles.map(roleLabel).join(", ")}</span>
          ) : undefined
        }
        aside={<a href="/foundry/exchange">Ask for an invite</a>}
      >
        {redeemed ? (
          <p className="fd-note" role="status">
            {redeemedName ? (
              <>
                Role granted to <strong>{redeemedName}</strong>.
              </>
            ) : (
              <>
                Role granted to this session.{" "}
                <a href="/foundry/persona">Claim a persona</a> to keep it — and
                get a return code.
              </>
            )}{" "}
            Hosts schedule sessions on{" "}
            <a href="/foundry/sessions">the calendar</a>.
          </p>
        ) : null}
        <form className="fd-form fd-people__redeem" method="post" onSubmit={submitRedeem}>
          <input type="hidden" name="intent" value="redeem_invite" />
          <div className="fd-form__field">
            <label className="fd-form__label" htmlFor="fd-invite-code">
              Invite code
            </label>
            <input
              id="fd-invite-code"
              ref={codeRef}
              className="fd-form__input fd-people__code"
              name="code"
              type="text"
              autoComplete="off"
              required
            />
          </div>
          <label className="fd-people__consent">
            <input
              type="checkbox"
              name="consentSteward"
              checked={consent}
              onChange={(e) => setConsent(e.target.checked)}
            />
            <span>
              I accept the steward code — details on{" "}
              <a href="/foundry/stewardship">Stewardship</a>. A host invite
              needs this; any invite records it.
            </span>
          </label>
          <div className="fd-form__actions">
            <Button type="submit" variant="primary" size="sm" disabled={pending}>
              Redeem code
            </Button>
            {pending ? <Spinner size={16} /> : null}
          </div>
        </form>
      </FdSection>

      {canMint ? (
        <FdSection
          title="Mint an invite"
          sub="A one-time code for someone you want here. Every mint is a recorded action — who, role, note; never the code."
        >
          {minted ? (
            <div className="fd-people__minted" role="status">
              <p className="fd-label">{roleLabel(minted.role)} invite</p>
              <code className="fd-people__mintcode">{minted.code}</code>
              <p className="fd-note">
                Shown once — copy it now. Whoever redeems it first, on this
                page, holds the {roleLabel(minted.role)} role; it cannot be
                read back later.
              </p>
            </div>
          ) : null}
          {mintError ? (
            <p className="fd-alert" role="alert">
              {mintError}
            </p>
          ) : null}
          <form
            className="fd-form fd-people__mint"
            method="post"
            onSubmit={submitMint}
          >
            <input type="hidden" name="intent" value="mint_invite" />
            <div className="fd-form__row">
              <div className="fd-form__field">
                <label className="fd-form__label" htmlFor="fd-mint-role">
                  Role
                </label>
                <select
                  id="fd-mint-role"
                  className="fd-form__select"
                  name="role"
                  defaultValue="start"
                >
                  {MINTABLE_ROLES.map((r) => (
                    <option key={r} value={r}>
                      {roleLabel(r)}
                    </option>
                  ))}
                </select>
              </div>
              <div className="fd-form__field">
                <label
                  className="fd-form__label"
                  htmlFor="fd-mint-expires"
                  title="The code stops working at the start of this day, UTC."
                >
                  Expires at start of day, UTC (optional)
                </label>
                <input
                  id="fd-mint-expires"
                  className="fd-form__input"
                  name="expires"
                  type="date"
                />
              </div>
            </div>
            <div className="fd-form__field">
              <label className="fd-form__label" htmlFor="fd-mint-note">
                Note (optional)
              </label>
              <input
                id="fd-mint-note"
                className="fd-form__input"
                name="note"
                type="text"
                autoComplete="off"
                placeholder="who this is for — lands in the ledger"
              />
            </div>
            <div className="fd-form__actions">
              <Button
                type="submit"
                variant="secondary"
                size="sm"
                disabled={mintPending || !onMint}
              >
                Mint invite
              </Button>
              {mintPending ? <Spinner size={16} /> : null}
            </div>
          </form>
        </FdSection>
      ) : null}

      <FdSection title="On the site now">
        {presence === null ? (
          <p className="fd-empty">The room server could not be read.</p>
        ) : presence.length === 0 ? (
          <p className="fd-empty">Nobody is connected to a page room right now.</p>
        ) : (
          <ul className="fd-people__presence">
            {presence.map((r) => (
              <li key={r.path}>
                <a href={r.path}>{r.path.replace(/^\/foundry\/?/, "") || "the front door"}</a>{" "}
                <span className="fd-num">{r.count}</span>
              </li>
            ))}
          </ul>
        )}
      </FdSection>

      <FdSection
        title="Directory"
        badge={
          people.length > 0 ? (
            <span className="fd-chip">{plural(people.length, "persona")}</span>
          ) : undefined
        }
      >
        {people.length === 0 ? (
          <p className="fd-empty">Nobody has claimed a persona yet.</p>
        ) : (
          <div className="fd-board">
            {people.map((p, i) => (
              <article className="fd-card" key={i}>
                <FdPersonaChip
                  actor={p.actor}
                  showAvatar
                  href={
                    "name" in p.actor
                      ? `/foundry/people/${encodeURIComponent(p.actor.name)}`
                      : undefined
                  }
                />
                {p.words ? <p className="fd-people__words">{p.words}</p> : null}
                {p.roles.length > 0 ? (
                  <p className="fd-chiprow">
                    {p.roles.map((r) => (
                      <span key={r} className="fd-chip">
                        {roleLabel(r)}
                      </span>
                    ))}
                  </p>
                ) : null}
                <dl className="fd-facts">
                  <div>
                    <dt>Claimed</dt>
                    <dd>
                      <FdTime iso={p.claimedAt} title={p.claimedAt}>
                        {dayUTC(p.claimedAt)}
                      </FdTime>
                    </dd>
                  </div>
                  <div>
                    <dt>Last seen</dt>
                    <dd>
                      {p.lastSeen === null ? (
                        <span className="fd-note-inline">no recorded action</span>
                      ) : (
                        <FdTime iso={p.lastSeen} title={p.lastSeen}>
                          {dayUTC(p.lastSeen)}
                        </FdTime>
                      )}
                    </dd>
                  </div>
                </dl>
                <div className="fd-card__foot">
                  <FdCountChip
                    count={p.requests}
                    ids={p.requestIds}
                    noun="request"
                  />
                  <FdCountChip
                    count={p.pledges}
                    ids={p.pledgeRequestIds}
                    noun="pledge"
                  />
                </div>
              </article>
            ))}
          </div>
        )}
      </FdSection>

      <FdSection
        title="Roster"
        badge={
          roster.rows.length > 0 ? (
            <span className="fd-chip">
              {plural(roster.rows.length, "holder")}
            </span>
          ) : undefined
        }
        sub={
          <>
            Role holders who consented to be listed — the consent lives on{" "}
            <a href="/foundry/stewardship">Stewardship</a>.
          </>
        }
      >
        {roster.rows.length === 0 ? (
          <p className="fd-empty">
            {roster.notListed === 0
              ? "No operator or host role has been granted yet."
              : `${plural(roster.notListed, "role holder")} exist, none consented to be listed.`}
          </p>
        ) : (
          <>
            <ul className="fd-people__roster">
              {roster.rows.map((r, i) => (
                <li className="fd-people__rosterrow" key={i}>
                  <FdPersonaChip actor={r.actor} />
                  <span className="fd-chip">{roleLabel(r.role)}</span>
                  <FdTime
                    iso={r.since}
                    className="fd-note-inline"
                    title={r.since}
                  >
                    granted {dayUTC(r.since)}
                  </FdTime>
                </li>
              ))}
            </ul>
            {roster.notListed > 0 ? (
              <p className="fd-note">
                {plural(roster.notListed, "role holder")} did not consent to be
                listed and are not named here.
              </p>
            ) : null}
          </>
        )}
      </FdSection>
    </div>
  );
}
