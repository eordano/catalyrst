import { useState } from "react";
import { GovernanceChromeMaybe, type GovernanceNavId } from "../frames/GovernanceChrome";
import Modal from "../../components/Modal";
import { Close, ChevronRight } from "../../atoms/icons";
import "./gvbidvotingflow.css";

function VotingDisabledIcon() {
  return (
    <svg viewBox="0 0 64 64" width="64" height="64" aria-hidden="true" className="gvbv__sr-icon">
      <circle cx="32" cy="32" r="26" fill="none" stroke="var(--black-400)" strokeWidth="4" />
      <path d="M14 14L50 50" stroke="var(--black-400)" strokeWidth="4" strokeLinecap="round" />
      <path d="M24 33l6 6 12-14" fill="none" stroke="var(--black-300)" strokeWidth="4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function SnapshotMark() {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
      <path d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" fill="currentColor" />
    </svg>
  );
}

const USD = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
});

export type GvBid = {
  id: string;
  title: string;
  budget: number;
  power: number;
  choice: string;
  current: boolean;
};

type Bid = GvBid;

function BidCard({ bid }: { bid: Bid }) {
  return (
    <a
      className={"gvbv__card" + (bid.current ? " is-disabled" : "")}
      href={bid.current ? undefined : `/governance/proposals/${bid.id}`}
    >
      <div className="gvbv__card-body">
        <span className="gvbv__card-title">{bid.title}</span>
        <div className="gvbv__card-details">
          <span>{USD.format(bid.budget)}</span>
          <span className="gvbv__dot">&#xB7;</span>
          <span>
            {bid.power.toLocaleString("en-US")} VP in {bid.choice}
          </span>
        </div>
      </div>
      <ChevronRight size={18} className="gvbv__chev" />
    </a>
  );
}

type GvBidVotingFlowProps = {
  chrome?: boolean;
  state?: string;
  vote?: string;
  retryTimer?: string;
  bids?: Bid[];
};

export default function GvBidVotingFlow({
  chrome = true,
  state = "default",
  vote = "",
  retryTimer = "30s",
  bids = [],
}: GvBidVotingFlowProps) {
  const [active, setActive] = useState<GovernanceNavId>("proposals");
  const isRedirect = state === "redirect";
  const isCasting = state === "casting";
  const isError = state === "error";

  return (
    <GovernanceChromeMaybe chrome={chrome} active={active} onTab={setActive}>
      <div className="gvbv">
        <Modal width="100%" className="modal__card--plain gvbv__modal" ariaLabel="Bid voting">
            <button type="button" className="gvbv__close" aria-label="Close">
              <Close size={14} strokeWidth={2.2} />
            </button>

            {!isRedirect ? (
              <>
                <div className="gvbv__content">
                  <div className="gvbv__title">
                    <h2 className="gvbv__heading">There&apos;s still more reviewing to do</h2>
                    <p className="gvbv__description">
                      The best way to vote is from consciousness.
                    </p>
                    <p className="gvbv__description">
                      Please, make sure you go through all {bids.length} proposals.
                    </p>
                  </div>

                  {bids.map((bid) => (
                    <BidCard key={bid.id} bid={bid} />
                  ))}
                </div>

                <div className="gvbv__actions">
                  <button
                    type="button"
                    className={"gvbv__action" + (isCasting ? " is-loading" : "")}
                    disabled={isError || isCasting}
                  >
                    {isCasting && <span className="gvbv__spinner" aria-hidden="true" />}
                    <span className="gvbv__action-label">
                      {isError
                        ? `Retry in ${retryTimer}...`
                        : vote
                          ? `Vote "${vote}" anyway`
                          : "Vote anyway"}
                    </span>
                  </button>
                </div>
              </>
            ) : (
              <div className="gvbv__content">
                <div className="gvbv__sr-body">
                  <VotingDisabledIcon />
                  <span className="gvbv__sr-header">Voting is not available</span>
                  <div className="gvbv__sr-desc">
                    You can still cast your vote directly on Snapshot.
                  </div>
                  <p className="gvbv__sr-suggestion">
                    If you think this is a mistake, please reach out to the DAO committee.
                  </p>
                </div>
                <div className="gvbv__sr-actions">
                  <a className="gvbv__sr-btn" href="https://snapshot.org/#/dao-council.dcl.eth" target="_blank" rel="noopener noreferrer">
                    <SnapshotMark />
                    Vote on Snapshot
                  </a>
                </div>
              </div>
            )}
        </Modal>
      </div>
    </GovernanceChromeMaybe>
  );
}
