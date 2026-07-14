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
  manaPerItem: number;
  mana: number;
};
type CwpcFee = {
  lines: CwpcFeeLine[];
  itemCount: number;
  manaPerItem: number;
  totalMana: number;
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
          <p className="cwpc__eyebrow cwpc__eyebrow--summary">Step 1 of 4 &#xB7; Review collection</p>
          <ChCollectionDetail
            collection={summary.collection}
            wearables={summary.wearables}
            emotes={summary.emotes}
            onPublish={onNext}
          />
          <div className="cwpc__controls" role="group" aria-label="Review collection">
            <Button variant="primary" onClick={() => onNext?.()}>
              Continue to publish fee
            </Button>
          </div>
        </>
      )}

      {view === "cost" && (
        <>
          <section className="cwpc__panel" aria-labelledby={costTitleId}>
            <p className="cwpc__eyebrow">Step 2 of 4 &#xB7; Publish fee</p>
            <h1 id={costTitleId} className="cwpc__title">
              Publish-fee breakdown
            </h1>
            <p className="cwpc__lead">
              Publishing charges a one-time MANA fee per item ({fee.manaPerItem} MANA
              each). The fee is the same for every rarity tier and is rolled up
              below.
            </p>
            <FeeTable fee={fee} />
          </section>
          <div className="cwpc__controls" role="group" aria-label="Publish fee">
            <Button variant="secondary" onClick={() => onBack?.()}>
              Back
            </Button>
            <Button variant="primary" onClick={() => onNext?.()}>
              Continue to terms
            </Button>
          </div>
        </>
      )}

      {view === "terms" && (
        <>
          <section className="cwpc__panel" aria-labelledby={termsTitleId}>
            <p className="cwpc__eyebrow">Step 3 of 4 &#xB7; Terms</p>
            <h1 id={termsTitleId} className="cwpc__title">
              Content &amp; curation terms
            </h1>
            <p className="cwpc__lead">
              Once submitted, the collection is reviewed by the Decentraland
              curation committee and its items are locked.
            </p>
            <div className="cwpc__terms" tabIndex={0}>
              <h2>By publishing this collection you confirm that:</h2>
              <ul>
                <li>You own or have the rights to all content in the collection.</li>
                <li>
                  The items comply with the Decentraland Content Policy and Code of
                  Ethics.
                </li>
                <li>
                  Items cannot be added or removed after publishing, and the
                  collection is locked pending curation review.
                </li>
                <li>
                  The MANA publish fee is non-refundable once the payment is
                  signed.
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
              disabled={!accepted}
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
          <p className="cwpc__eyebrow">Step 4 of 4 &#xB7; Payment</p>
          <div className="cwpc__spinner" aria-hidden="true" />
          <h1 id={payTitleId} className="cwpc__title">
            Approve MANA &amp; sign publish
          </h1>
          <p className="cwpc__lead">
            Confirm the {fee.totalMana} MANA publish fee in your wallet. The on-chain
            payment is <strong>simulated</strong> on this realm.
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
            Submitted for curation review
          </h1>
          <p className="cwpc__lead">
            {collectionName ? <>&#x201C;{collectionName}&#x201D;</> : "Your collection"}{" "}
            ({fee.itemCount} item{fee.itemCount === 1 ? "" : "s"},
            {" "}
            {fee.totalMana} MANA) is locked and now in the curation queue. On
            mainnet the committee reviews it and posts its decision on the
            collection's forum topic &#x2014; typically within days, sometimes weeks.
            Here the committee review is a <strong>stub</strong> on this realm,
            so no real review or forum post will happen.
          </p>
          {statusHref ? (
            <p className="cwpc__lead">
              <a className="cwpc__statuslink" href={statusHref}>
                Track your submission
              </a>{" "}
              &#x2014; a local status view of this simulated review.
            </p>
          ) : null}
          {txHash ? (
            <p className="cwpc__tx">tx: {txHash} (simulated)</p>
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
              Publish payment failed
            </h1>
            <p className="cwpc__lead">
              {error || "The publish payment could not be completed."}{" "}
              You can try the (simulated) payment again or go back to the terms.
            </p>
          </section>
          <div className="cwpc__controls">
            <Button variant="secondary" onClick={() => onBack?.()}>
              Back to terms
            </Button>
            <Button variant="primary" onClick={() => onRetry?.()}>
              Try again
            </Button>
          </div>
        </>
      )}

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

function FeeTable({ fee }: { fee: CwpcFee }) {
  return (
    <table className="cwpc__fee">
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
                {line.manaPerItem}
              </span>
            </td>
            <td className="cwpc__num">
              <span className="cwpc__mana">
                <ManaGlyph />
                {line.mana}
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
              {fee.totalMana}
            </span>
          </td>
        </tr>
      </tbody>
    </table>
  );
}
