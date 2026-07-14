import { type FormEvent, useId, useState } from "react";

import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdTime from "../components/FdTime";
import { AvatarStage } from "../../explorer/components/AvatarPreview";
import {
  AVATAR_EYE_COLORS,
  AVATAR_HAIR_COLORS,
  AVATAR_SKIN_COLORS,
  type BodyId,
  type Color3,
} from "../../data/randomIdentity";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdPersonaChip, { type FdAvatarSpec, fdOutfit } from "../components/FdPersonaChip";
import FdReturnForm from "../components/FdReturnForm";
import { dayUTC } from "../fmt";
import "../components/fdpersonachip.css";
import "./fdpersona.css";

export type FdPersonaVM = {
  displayName: string;
  avatar: FdAvatarSpec | null;
  /** The visitor's own self-description; null = never written. */
  words: string | null;
  claimedAt: string;
  updatedAt: string;
};

export type FdPersonaHistoryItem = { at: string; name: string };

export type FdPersonaFormValues = {
  displayName: string;
  words: string;
  body: BodyId;
  skin: number;
  hair: number;
  eyes: number;
};

export type FdCarryStatusVM = {
  /** Mint time of the active carry code, or null when none stands. */
  activeSince: string | null;
  /** When a code of this persona's was last redeemed, or null. */
  lastRedeemedAt: string | null;
};

export type FdPersonaPageProps = {
  persona: FdPersonaVM | null;
  badge: string;
  history: readonly FdPersonaHistoryItem[];
  error?: string | null;
  /** True right after a successful claim/save — the page shows the onward doors. */
  saved?: boolean;
  pending?: boolean;
  /** Carry-code facts for the holder; null while no persona is claimed. */
  carry?: FdCarryStatusVM | null;
  /** The just-minted one-time code — shown once, never re-readable. */
  carryCode?: string | null;
  /** The one-time code minted with the claim itself — same contract as
   *  carryCode, delivered on the save response. */
  returnCode?: string | null;
  /** False when no unspent code stands for this persona — the page then leads
   *  with the mint banner, so recovery is armed before it is needed. */
  hasReturnCode?: boolean;
  /** True when the mint superseded an earlier active code. */
  carryReplaced?: boolean;
  carryError?: string | null;
  carryPending?: boolean;
  /** Set right after a successful redeem, while the page reloads. */
  redeemedName?: string | null;
  onMintCarry?: () => void;
  onRedeemCarry?: (code: string) => void;
  onSubmit: (values: FdPersonaFormValues) => void;
};

function rgb(c: Color3): string {
  const to = (v: number) => Math.max(0, Math.min(255, Math.round(v * 255)));
  return `rgb(${to(c.r)}, ${to(c.g)}, ${to(c.b)})`;
}

const DEFAULT_SPEC: FdAvatarSpec = { body: "A", skin: 0, hair: 0, eyes: 0 };

function Swatches({
  colors,
  value,
  onPick,
  label,
}: {
  colors: readonly Color3[];
  value: number;
  onPick: (i: number) => void;
  label: string;
}) {
  const labelId = useId();
  return (
    <div className="fd-persona__field">
      <span className="fd-label" id={labelId}>
        {label}
      </span>
      <div className="fd-persona__swatches" role="group" aria-labelledby={labelId}>
        {colors.map((c, i) => (
          <button
            key={i}
            type="button"
            className={"fd-persona__sw" + (i === value ? " is-on" : "")}
            style={{ background: rgb(c) }}
            aria-pressed={i === value}
            aria-label={`${label} ${i + 1}`}
            onClick={() => onPick(i)}
          />
        ))}
      </div>
    </div>
  );
}

export default function FdPersonaPage({
  persona,
  badge,
  history,
  error = null,
  saved = false,
  pending = false,
  carry = null,
  carryCode = null,
  returnCode = null,
  hasReturnCode = true,
  carryReplaced = false,
  carryError = null,
  carryPending = false,
  redeemedName = null,
  onMintCarry,
  onRedeemCarry,
  onSubmit,
}: FdPersonaPageProps) {
  // The last code the holder confirmed saving — the panel stays until then,
  // and a fresh mint brings it back for the new code.
  const [savedCode, setSavedCode] = useState<string | null>(null);
  const oneTimeCode = returnCode ?? carryCode;
  const showCode = oneTimeCode !== null && oneTimeCode !== savedCode;
  const initial = persona?.avatar ?? DEFAULT_SPEC;
  const [name, setName] = useState(persona?.displayName ?? "");
  const [words, setWords] = useState(persona?.words ?? "");
  const [body, setBody] = useState<BodyId>(initial.body);
  const [skin, setSkin] = useState(initial.skin);
  const [hair, setHair] = useState(initial.hair);
  const [eyes, setEyes] = useState(initial.eyes);
  const bodyLabelId = useId();

  const spec: FdAvatarSpec = { body, skin, hair, eyes };
  const chipActor = name.trim()
    ? { name: name.trim(), avatar: spec }
    : { badge };
  // Right after a claim the loader data may not have caught up yet — the typed
  // name is the persona's name in that window.
  const personaName = persona?.displayName ?? name.trim();

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    onSubmit({ displayName: name.trim(), words: words.trim(), body, skin, hair, eyes });
  }

  return (
    <div className="fd-page fd-stack fd-persona">
      <FdPageHead
        title="Your persona"
        intro="A name and an avatar, kept to this browser's session cookie — no wallet, no email, no account. A return code brings it back in any browser."
      />

      {saved ? (
        <p className="fd-note" role="status">
          Saved. <a href="/foundry/people">You are listed in People →</a>{" "}
          <a href="/foundry/exchange">Post your first ask on the Exchange →</a>
        </p>
      ) : null}

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      <FdSection title={persona ? "Edit your persona" : "Claim a persona"}>
        <form className="fd-form fd-persona__form" method="post" onSubmit={submit}>
          <input type="hidden" name="body" value={body} />
          <input type="hidden" name="skin" value={String(skin)} />
          <input type="hidden" name="hair" value={String(hair)} />
          <input type="hidden" name="eyes" value={String(eyes)} />
          <div className="fd-persona__grid">
            <div className="fd-persona__preview">
              <AvatarStage
                className="fd-persona__stage"
                label="Your avatar, rendered live"
                outfit={fdOutfit(spec)}
                pauseOffscreen
              />
              <p className="fd-persona__chipdemo">
                <FdPersonaChip actor={chipActor} showAvatar />
                <span className="fd-note-inline">
                  the chip beside your posts
                </span>
              </p>
            </div>

            <div className="fd-persona__controls">
              <div className="fd-form__field">
                <label className="fd-form__label" htmlFor="fd-persona-name">
                  Display name
                </label>
                <input
                  id="fd-persona-name"
                  className="fd-form__input"
                  name="displayName"
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  minLength={2}
                  maxLength={32}
                  pattern="[A-Za-z0-9 ._\-]{2,32}"
                  title="2–32 characters. Letters, numbers, spaces and . _ - only."
                  required
                  autoComplete="off"
                />
                <p className="fd-note">
                  2–32 characters. Letters, numbers, spaces and . _ - only.
                </p>
              </div>

              <div className="fd-form__field">
                <label className="fd-form__label" htmlFor="fd-persona-words">
                  In your own words
                </label>
                <textarea
                  id="fd-persona-words"
                  className="fd-form__input"
                  name="words"
                  value={words}
                  onChange={(e) => setWords(e.target.value)}
                  maxLength={280}
                  rows={3}
                  placeholder="Who you are here — optional, shown on People."
                />
              </div>

              <div className="fd-persona__field">
                <span className="fd-label" id={bodyLabelId}>
                  Body
                </span>
                <div
                  className="fd-persona__bodies"
                  role="group"
                  aria-labelledby={bodyLabelId}
                >
                  {(["A", "B"] as const).map((b) => (
                    <Button
                      key={b}
                      variant={b === body ? "secondary" : "ghost"}
                      size="sm"
                      aria-pressed={b === body}
                      onClick={() => setBody(b)}
                    >
                      {b === "A" ? "Base A" : "Base B"}
                    </Button>
                  ))}
                </div>
              </div>

              <Swatches colors={AVATAR_SKIN_COLORS} value={skin} onPick={setSkin} label="Skin" />
              <Swatches colors={AVATAR_HAIR_COLORS} value={hair} onPick={setHair} label="Hair" />
              <Swatches colors={AVATAR_EYE_COLORS} value={eyes} onPick={setEyes} label="Eyes" />

              <div className="fd-form__actions">
                <Button type="submit" variant="primary" size="md" disabled={pending}>
                  {persona ? "Save changes" : "Claim this persona"}
                </Button>
                {pending ? <Spinner size={16} /> : null}
              </div>
            </div>
          </div>
        </form>
      </FdSection>

      {persona ? (
        <FdSection title="Return code">
          {showCode ? (
            <div className="fd-persona__carry" role="status">
              <p className="fd-label">Your return code</p>
              <code className="fd-persona__code">{oneTimeCode}</code>
              <p className="fd-note">
                Shown once. If a browser forgets you, enter it at{" "}
                <strong>foundry.catalyst.example.com/foundry/return</strong> and you're{" "}
                <strong>{personaName}</strong> again — role, acts, and all.
                Anyone holding it can be you, so keep it like the invite that
                brought you here.
                {carryReplaced ? " Your previous code no longer works." : ""}
              </p>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setSavedCode(oneTimeCode)}
              >
                I saved it
              </Button>
            </div>
          ) : !hasReturnCode ? (
            <div className="fd-persona__carry">
              <p className="fd-note">
                This persona has no return code yet. Mint one now — shown once
                — so a cleared cookie can't take <strong>{personaName}</strong>{" "}
                from you. Minting a new code retires the old one.
              </p>
              <Button
                variant="secondary"
                size="sm"
                onClick={onMintCarry}
                disabled={carryPending || !onMintCarry}
              >
                Mint return code
              </Button>
            </div>
          ) : (
            <div className="fd-persona__carry">
              {carry?.activeSince ? (
                <p className="fd-note">
                  A return code stands, minted{" "}
                  <FdTime iso={carry.activeSince} title={carry.activeSince}>
                    {dayUTC(carry.activeSince)}
                  </FdTime>
                  . It brings this persona back after a lost cookie and moves
                  it to another browser. Minting a new code retires the old
                  one.
                </p>
              ) : null}
              <Button
                variant="secondary"
                size="sm"
                onClick={onMintCarry}
                disabled={carryPending || !onMintCarry}
              >
                Mint a new return code
              </Button>
            </div>
          )}
          {carry?.lastRedeemedAt ? (
            <p className="fd-note">
              A code of yours was last redeemed{" "}
              <FdTime iso={carry.lastRedeemedAt} title={carry.lastRedeemedAt}>
                {dayUTC(carry.lastRedeemedAt)}
              </FdTime>
              .
            </p>
          ) : null}
          {carryError ? (
            <p className="fd-alert" role="alert">
              {carryError}
            </p>
          ) : null}
        </FdSection>
      ) : (
        <FdSection title="Bring a persona from another browser">
          <p className="fd-note">
            If you claimed a persona elsewhere, mint a return code there (Your
            persona → Return code) and enter it here. This browser becomes that
            persona — role, acts, and all.
          </p>
          <FdReturnForm
            pending={carryPending}
            error={carryError}
            redeemedName={redeemedName}
            submitLabel="Redeem"
            onRedeem={onRedeemCarry}
          />
        </FdSection>
      )}

      {persona ? (
        <FdSection title="What is stored">
          <dl className="fd-facts">
            <div>
              <dt>Claimed</dt>
              <dd>
                <FdTime iso={persona.claimedAt} title={persona.claimedAt}>
                  {dayUTC(persona.claimedAt)}
                </FdTime>
              </dd>
            </div>
            {persona.updatedAt !== persona.claimedAt ? (
              <div>
                <dt>Last changed</dt>
                <dd>
                  <FdTime iso={persona.updatedAt} title={persona.updatedAt}>
                    {dayUTC(persona.updatedAt)}
                  </FdTime>
                </dd>
              </div>
            ) : null}
            <div>
              <dt>Verified</dt>
              <dd>nothing is verified, and no wallet is attached</dd>
            </div>
          </dl>
        </FdSection>
      ) : null}

      {history.length > 0 ? (
        <FdSection title="Name history">
          <ul className="fd-persona__history">
            {history.map((h, i) => (
              <li className="fd-persona__historyrow" key={i}>
                <FdTime iso={h.at} className="fd-persona__historyat" title={h.at}>
                  {dayUTC(h.at)}
                </FdTime>
                <span className="fd-persona__historyname">{h.name}</span>
              </li>
            ))}
          </ul>
        </FdSection>
      ) : null}
    </div>
  );
}
