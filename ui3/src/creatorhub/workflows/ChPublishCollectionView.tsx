import { useId } from "react";
import ChCollectionDetail from "../pages/ChCollectionDetail";
import type { ChCollection, ChCollectionItem } from "../pages/ChCollectionDetail";
import Button from "../../atoms/Button";
import "./chpublishcollectionview.css";

type CwpcSummary = {
  collection: ChCollection;
  wearables: ChCollectionItem[];
  emotes: ChCollectionItem[];
};
type CwpcFeeLine = {
  rarity: string;
  count: number;
  manaPerItem: number | string;
  mana: number | string;
};
type CwpcFee = {
  lines: CwpcFeeLine[];
  itemCount: number;
  manaPerItem: number | string;
  totalMana: number | string;
};

type ChPublishCollectionViewProps = {
  step?: string;
  view?: string;
  collectionName?: string;
  summary?: CwpcSummary | null;
  fee?: CwpcFee;
  txHash?: string;
  error?: string;
  statusHref?: string;
  accepted?: boolean;
  onAcceptedChange?: (accepted: boolean) => void;
  onNext?: () => void;
  onBack?: () => void;
  onAccept?: () => void;
  onRetry?: () => void;
  onDone?: () => void;
  live?: boolean;
  linked?: { providerName: string; availableSlots: number | null; requiredSlots: number };
  email?: string;
  onEmailChange?: (email: string) => void;
  forumUrl?: string;
  progress?: string;
  resumeMessage?: string;
  recoveryHash?: string;
  onRecoveryHashChange?: (hash: string) => void;
  onRecover?: () => void;
  onCancel?: () => void;
};

export default function ChPublishCollectionView({
  step = "summary",
  view = "summary",
  collectionName = "",
  summary = null,
  fee = { lines: [], itemCount: 0, manaPerItem: 0, totalMana: 0 },
  txHash = "",
  error = "",
  statusHref = "",
  accepted = false,
  onAcceptedChange = undefined,
  onNext = undefined,
  onBack = undefined,
  onAccept = undefined,
  onRetry = undefined,
  onDone = undefined,
  linked, live = false, email = "", onEmailChange, forumUrl, progress, resumeMessage,
  recoveryHash = "", onRecoveryHashChange, onRecover, onCancel,
}: ChPublishCollectionViewProps) {
  const costTitleId = useId();
  const termsTitleId = useId();
  const payTitleId = useId();
  const doneTitleId = useId();
  const errTitleId = useId();
  const blockedTitleId = useId();

  return (
    <div className="cwpc" data-step={step}>
      {view === "summary" && summary && (
        <>
          <ChCollectionDetail
            collection={summary.collection}
            wearables={summary.wearables}
            emotes={summary.emotes}
            onPublish={onNext}
          />
          <div className="cwpc__controls" role="group" aria-label="Review collection">
            <Button variant="primary" onClick={() => onNext?.()}>
              {linked ? "Review item slots" : "Continue to publish fee"}
            </Button>
          </div>
        </>
      )}

      {view === "cost" && (
        <>
          <section className="cwpc__panel" aria-labelledby={costTitleId}>
            <h1 id={costTitleId} className="cwpc__title">
              {linked ? "Publish linked items" : "Publish-fee breakdown"}
            </h1>
            <p className="cwpc__lead">
              {linked ? <>Publishing uses item slots from {linked.providerName}. You authorize the slots with your wallet.</> : <>
                The publication fee is paid in MANA on Polygon. Network gas is separate.
                {live ? " These fees come from the collection contract and are checked again before payment." : " This preview uses sample fees."}
              </>}
            </p>
            {linked ? <>
              <dl className="cwpc__slots">
                <div><dt>Items to publish</dt><dd>{linked.requiredSlots.toLocaleString()}</dd></div>
                <div><dt>Available slots</dt><dd>{linked.availableSlots?.toLocaleString() ?? "\u2014"}</dd></div>
              </dl>
              {linked.availableSlots !== null && linked.availableSlots < linked.requiredSlots && <p className="cwpc__lead" role="alert">Your provider needs more item slots before you can publish this collection.</p>}
            </> : <FeeTable fee={fee} />}
          </section>
          <div className="cwpc__controls" role="group" aria-label="Publish fee">
            <Button variant="secondary" onClick={() => onBack?.()}>
              Back
            </Button>
            <Button variant="primary" disabled={!!linked && (linked.availableSlots === null || linked.availableSlots < linked.requiredSlots)} onClick={() => onNext?.()}>
              Continue to terms
            </Button>
          </div>
        </>
      )}

      {view === "terms" && (
        <>
          <section className="cwpc__panel" aria-labelledby={termsTitleId}>
            <h1 id={termsTitleId} className="cwpc__title">
              Content &amp; curation terms
            </h1>
            <p className="cwpc__lead">
              Once submitted, the collection is reviewed by the Decentraland
              curation committee and its items are locked.
            </p>
            {live && <label className="cwpc__field">Email for publication terms
              <input type="email" autoComplete="email" required maxLength={254} value={email} onChange={e => onEmailChange?.(e.target.value)} />
              <span>Shared with Decentraland Foundation for this submission.</span>
            </label>}
            <div className="cwpc__terms" tabIndex={0}>
              <h2>By publishing this collection you confirm that:</h2>
              <ul>
                <li>You own or have the rights to all content in the collection.</li>
                <li>
                  The items comply with the Decentraland Content Policy and Code of
                  Ethics.
                </li>
                <li>
                  {linked ? "These items are locked while Foundation reviews their content." : "Items cannot be added or removed after publishing, and the collection is locked pending curation review."}
                </li>
                <li>
                  {linked ? <>You authorize {linked.requiredSlots.toLocaleString()} item slot{linked.requiredSlots === 1 ? "" : "s"} from {linked.providerName} for this submission.</> : "The MANA publish fee is non-refundable once the payment is confirmed."}
                </li>
              </ul>
            </div>
            <label className="cwpc__check">
              <input
                type="checkbox"
                checked={accepted}
                onChange={(e) => onAcceptedChange?.(e.target.checked)}
              />
              <span>
                I have read and accept the content policy and curation terms above.
              </span>
            </label>
          </section>
          <div className="cwpc__controls" role="group" aria-label="Accept terms">
            <Button variant="secondary" onClick={() => onBack?.()}>
              Back
            </Button>
            <Button
              variant="primary"
              disabled={!accepted || (live && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()))}
              aria-label={accepted ? undefined : "Check the box to continue"}
              title={accepted ? undefined : "Check the box to continue"}
              onClick={() => onAccept?.()}
            >
              Accept &amp; continue
            </Button>
          </div>
        </>
      )}

      {view === "pay" && (
        <section className="cwpc__status" aria-labelledby={payTitleId} aria-live="polite">
          <div className="cwpc__spinner" aria-hidden="true" />
          <h1 id={payTitleId} className="cwpc__title">
            {progress || (linked ? "Authorize item slots" : "Approve MANA & sign publish")}
          </h1>
          <p className="cwpc__lead">
            {linked ? "Follow the prompts in your wallet. Retrying uses the same item-slot authorization." : live ? "Follow the prompts in your wallet. Once paid, retries continue the same submission without another publication payment." : <>Confirm the {fee.totalMana} MANA sample fee. This payment is simulated.</>}
          </p>
        </section>
      )}

      {view === "submitted" && (
        <section className="cwpc__status" aria-labelledby={doneTitleId}>
          <svg className="cwpc__bigcheck" viewBox="0 0 64 64" width="64" height="64" aria-hidden="true">
            <circle cx="32" cy="32" r="29" fill="none" stroke="currentColor" strokeWidth="4" />
            <path d="M20 33l8 8 16-18" fill="none" stroke="currentColor" strokeWidth="4" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <h1 id={doneTitleId} className="cwpc__title">
            {linked ? "Sent to Foundation" : "Submitted for curation review"}
          </h1>
          <p className="cwpc__lead">
            {collectionName ? <>&#x201C;{collectionName}&#x201D;</> : "Your collection"}{" "}
            {linked ? <>was submitted to the Decentraland curation committee.{forumUrl ? " Follow the review on its forum topic." : " You can check its status in Foundation Builder."}</> : live ? <>is submitted to the Decentraland curation committee. Follow the review on its forum topic.</> : <>({fee.itemCount} item{fee.itemCount === 1 ? "" : "s"},
            {" "}
            {fee.totalMana} MANA) is locked and now in the curation queue. On
            mainnet the committee reviews it and posts its decision on the
            collection's forum topic &#x2014; typically within days, sometimes weeks.
            Here the committee review is a <strong>stub</strong> on this realm,
            so no real review or forum post will happen.</>}
          </p>
          {statusHref ? (
            <p className="cwpc__lead">
              <a className="cwpc__statuslink" href={statusHref}>
                Track your submission
              </a>{" "}
              {!live && <> &#x2014; a local status view of this simulated review.</>}
            </p>
          ) : null}
          {forumUrl && <p><a className="cwpc__statuslink" href={forumUrl} target="_blank" rel="noreferrer">Open review topic</a></p>}
          {txHash ? (
            <p className="cwpc__tx">tx: {live ? <a className="cwpc__statuslink" href={`https://polygonscan.com/tx/${txHash}`} target="_blank" rel="noreferrer">{txHash}</a> : <>{txHash} (simulated)</>}</p>
          ) : null}
          {onDone ? (
            <div className="cwpc__controls">
              <Button variant="primary" onClick={() => onDone?.()}>
                Back to collections
              </Button>
            </div>
          ) : null}
        </section>
      )}

      {view === "error" && (
        <>
          <section className="cwpc__status" role="alert" aria-labelledby={errTitleId}>
            <h1 id={errTitleId} className="cwpc__title">
              Publication needs attention
            </h1>
            <p className="cwpc__lead">
              {error || (linked ? "The linked submission could not be completed." : "The publish payment could not be completed.")}{" "}
              {live ? "Retry to continue from the last completed step." : "You can retry this simulated payment."}
            </p>
          </section>
          <div className="cwpc__controls">
            <Button variant="secondary" onClick={() => onBack?.()}>
              {live ? "Review collection" : "Back to terms"}
            </Button>
            <Button variant="primary" onClick={() => onRetry?.()}>
              Try again
            </Button>
          </div>
        </>
      )}

      {(view === "checking" || view === "resume") && <section className="cwpc__status" aria-live="polite">
        <h1 className="cwpc__title">{view === "checking" ? "Checking your collection" : "Continue your publication"}</h1>
        <p className="cwpc__lead">{view === "checking" ? (linked ? "Inspecting models and checking your provider's item slots." : "Inspecting models and checking the current publication fee.") : resumeMessage}</p>
        {view === "resume" && onRetry && <Button variant="primary" onClick={onRetry}>Continue submission</Button>}
      </section>}
      {(view === "error" || view === "resume") && (onRecover || onCancel) && <section className="cwpc__panel" aria-label="Recover publication">
        {onRecover && <>
          <label className="cwpc__field">Polygon publication transaction hash
            <input value={recoveryHash} onChange={e => onRecoveryHashChange?.(e.target.value)} placeholder="0x&#x2026;" autoComplete="off" spellCheck={false} />
            <span>Copy the collection transaction from your wallet activity.</span>
          </label>
          <Button variant="secondary" disabled={!/^0x[\da-f]{64}$/i.test(recoveryHash.trim())} onClick={onRecover}>Recover transaction</Button>
        </>}
        {onCancel && <Button variant="secondary" onClick={onCancel}>Unlock draft for editing</Button>}
      </section>}

      {view === "blocked" && (
        <section className="cwpc__blocked" aria-labelledby={blockedTitleId}>
          <svg viewBox="0 0 64 64" width="56" height="56" aria-hidden="true">
            <path d="M32 6l28 50H4L32 6z" fill="none" stroke="currentColor" strokeWidth="4" strokeLinejoin="round" />
            <path d="M32 24v14M32 46v.05" stroke="currentColor" strokeWidth="4.5" strokeLinecap="round" />
          </svg>
          <h1 id={blockedTitleId} className="cwpc__title">
            Nothing to publish yet
          </h1>
          <p className="cwpc__lead">
            {collectionName ? <>&#x201C;{collectionName}&#x201D; has no items. </> : null}
            Add at least one wearable or emote to the collection before
            publishing.
          </p>
        </section>
      )}
    </div>
  );
}

const RARITY_COLOR: Record<string, string> = {
  unique: "#fea217",
  mythic: "#ff4bed",
  exotic: "#9bd141",
  legendary: "#a755f4",
  epic: "#438fff",
  rare: "#34ce76",
  uncommon: "#ff8362",
  common: "#73d3d3",
};

function ManaGlyph() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
      <path d="M8 1.6L13 8 8 14.4 3 8 8 1.6z M8 4.4L5 8l3 3.6L11 8 8 4.4z" fill="currentColor" />
    </svg>
  );
}

function displayMana(value: number | string) {
  const text=String(value);
  const [whole,fraction=""]=text.split(".");
  return fraction.length>4 && /[1-9]/.test(fraction.slice(4)) ? `\u2248${whole}.${fraction.slice(0,4)}` : text;
}

function FeeTable({ fee }: { fee: CwpcFee }) {
  return (
    <><table className="cwpc__fee">
      <thead>
        <tr>
          <th>Rarity</th>
          <th className="cwpc__num">Items</th>
          <th className="cwpc__num">Fee / item</th>
          <th className="cwpc__num">Subtotal</th>
        </tr>
      </thead>
      <tbody>
        {fee.lines.map((line) => (
          <tr key={line.rarity}>
            <td>
              <span
                className="cwpc__rarity"
                style={{ background: RARITY_COLOR[line.rarity] }}
              >
                {line.rarity}
              </span>
            </td>
            <td className="cwpc__num">{line.count}</td>
            <td className="cwpc__num">
              <span className="cwpc__mana">
                <ManaGlyph />
                {displayMana(line.manaPerItem)}
              </span>
            </td>
            <td className="cwpc__num">
              <span className="cwpc__mana">
                <ManaGlyph />
                {displayMana(line.mana)}
              </span>
            </td>
          </tr>
        ))}
        <tr className="cwpc__feetotal">
          <td>Total</td>
          <td className="cwpc__num">{fee.itemCount}</td>
          <td className="cwpc__num" />
          <td className="cwpc__num">
            <span className="cwpc__mana">
              <ManaGlyph />
              {displayMana(fee.totalMana)}
            </span>
          </td>
        </tr>
      </tbody>
    </table>
    <p className="cwpc__exact">Exact fee: <span>{fee.totalMana} MANA</span></p></>
  );
}
