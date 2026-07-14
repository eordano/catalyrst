import { useEffect, useMemo, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { Link, useSearchParams } from "react-router";

import GovernanceChrome, { type GovernanceNavId } from "@ui/governance/frames/GovernanceChrome";

import type { TrackContext } from "@core/lib/telemetry/track";
import type {
  Candidate,
  DelegateData,
} from "@data/lib/catalyst/governance/delegate-vp";
import { vpLabel } from "@data/lib/catalyst/governance/delegate-vp";
import { buildDelegateVp } from "@data/lib/catalyst/governance/delegate-wallet";
import type { VpDistribution } from "@data/lib/catalyst/governance/snapshot-vp";
import {
  delegateMachine,
  resolveDelegateSnapshot,
  slugToState,
  stateToSlug,
  type DelegateFn,
  type TrackFn,
} from "./machine";

export type DelegateVpWizardProps = {
  data: DelegateData;
  trackCtx: TrackContext;
  initialStep?: string;
  initialCandidate?: string;
  delegate?: DelegateFn;
  track?: TrackFn;
};

const DISTRIBUTION: { key: keyof VpDistribution; label: string }[] = [
  { key: "own", label: "Own" },
  { key: "delegated", label: "Delegated to them" },
  { key: "mana", label: "MANA" },
  { key: "wMana", label: "Wrapped MANA" },
  { key: "names", label: "NAMES" },
  { key: "l1Wearables", label: "L1 wearables" },
  { key: "land", label: "LAND" },
  { key: "estate", label: "Estates" },
  { key: "rental", label: "LAND rental" },
];

export default function DelegateVpWizard({
  data,
  trackCtx,
  initialStep,
  initialCandidate,
  delegate,
  track,
}: DelegateVpWizardProps) {
  const [searchParams] = useSearchParams();
  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const urlCandidate =
    (searchParams.get("candidate")?.trim() || initialCandidate) ?? undefined;
  const stateId = slugToState(urlStep);

  const seeded =
    data.candidates.find((c) => c.id === urlCandidate) ?? data.candidates[0];

  const key = `${stateId}:${seeded?.id ?? ""}`;

  return (
    <DelegateVpWizardInner
      key={key}
      stateId={stateId}
      seeded={seeded}
      data={data}
      trackCtx={trackCtx}
      delegate={delegate}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  seeded: Candidate | undefined;
  data: DelegateData;
  trackCtx: TrackContext;
  delegate?: DelegateFn;
  track?: TrackFn;
};

function DelegateVpWizardInner({
  stateId,
  seeded,
  data,
  trackCtx,
  delegate,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const [tab, setTab] = useState<GovernanceNavId>("proposals");

  const submit = useMemo(
    () =>
      delegate ??
      buildDelegateVp({ registry: data.registry, currentDelegate: data.delegatedTo }),
    [delegate, data.registry, data.delegatedTo],
  );

  const candidateSeed =
    seeded && stateId !== "browsing"
      ? { id: seeded.id, address: seeded.address, name: seeded.name }
      : undefined;

  const snapshot = useRef(
    resolveDelegateSnapshot({
      step: stateId,
      trackCtx,
      space: data.space,
      vp: data.userVp,
      delegate: submit,
      track,
      candidate: candidateSeed,
    }),
  ).current;

  const [state, send] = useMachine(delegateMachine, {
    input: {
      trackCtx,
      space: data.space,
      vp: data.userVp,
      delegate: submit,
      track,
      candidateId: candidateSeed?.id,
      candidateAddress: candidateSeed?.address,
      candidateName: candidateSeed?.name,
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const activeId = state.context.candidateId ?? seeded?.id;
  const candidate = useMemo(
    () => data.candidates.find((c) => c.id === activeId),
    [data.candidates, activeId],
  );
  const receipt = state.context.receipt;

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const syncKey = `${step}|${activeId ?? ""}`;
    if (lastKey.current === syncKey) return;
    lastKey.current = syncKey;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        let changed = false;
        if (params.get("step") !== step) {
          params.set("step", step);
          changed = true;
        }
        if (step !== "browse" && activeId && params.get("candidate") !== activeId) {
          params.set("candidate", activeId);
          changed = true;
        }
        return changed ? params : prev;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, activeId, setSearchParams]);

  return (
    <GovernanceChrome active={tab} onTab={setTab}>
      <div className="gv-delegate-vp" data-step={step} data-source={data.source}>
        {value === "browsing" && (
          <section className="gv-delegate-vp__browse" style={WRAP}>
            <header style={{ marginBottom: 16 }}>
              <h1 style={H1}>Delegate your Voting Power</h1>
              <p style={SUB}>
                Delegation is an on-chain <code>setDelegate</code> call to the Snapshot
                delegate registry, signed by your own wallet. You keep your tokens and
                can re-delegate or vote yourself at any time.
              </p>
              <p style={SUB}>
                Your Voting Power in <code>{data.space}</code>:{" "}
                <strong>{data.userVpLabel}</strong>
                {data.userVp === null && data.address
                  ? " (live Snapshot voting power unavailable)"
                  : ""}
              </p>
              {data.delegatedTo && (
                <p style={SUB} data-testid="current-delegation">
                  Currently delegated to <code>{data.delegatedTo}</code>
                  {data.delegationScope === "global"
                    ? " (global delegation, all spaces)"
                    : ""}
                  .
                </p>
              )}
            </header>
            <Blockers items={data.blockers} />
            {data.candidates.length === 0 ? (
              <div style={EMPTY} role="status">
                <p style={{ margin: "0 0 8px", fontWeight: 600, color: "#fcfcfc" }}>
                  {data.needsWallet
                    ? "Sign in to delegate Voting Power"
                    : "No delegates to show"}
                </p>
                <p style={{ margin: "0 0 16px" }}>
                  {data.needsWallet
                    ? "Sign in with your wallet to see your Voting Power and your current delegation."
                    : "The governance archive returned no voters for this window."}
                </p>
                <Link to="/governance" prefetch="intent" style={EMPTY_LINK}>
                  Back to DAO Home
                </Link>
              </div>
            ) : (
              <>
                <p style={NOTE}>
                  These are the most active voters in <code>{data.space}</code> over the
                  last {data.rosterWindowDays} days, from the DAO vote archive. Voting
                  Power is read live from Snapshot. This is not a curated candidate list
                  and it is not an endorsement.
                </p>
                <ul style={CARD_GRID} aria-label="Delegates">
                  {data.cards.map((c) => (
                    <li key={c.id}>
                      <button
                        type="button"
                        style={CARD}
                        onClick={() => {
                          const full = data.candidates.find((x) => x.id === c.id);
                          send({
                            type: "PICK_CANDIDATE",
                            id: c.id,
                            address: full?.address ?? "",
                            name: c.name,
                          });
                        }}
                      >
                        <span
                          className="u-avatar"
                          style={{ "--sz": "44px", "--hue": c.hue } as React.CSSProperties}
                          aria-hidden="true"
                        />
                        <span style={{ flex: 1, textAlign: "left" }}>
                          <span style={CARD_NAME}>{c.name}</span>
                          <span style={CARD_ADDR}>{c.addressShort}</span>
                          <span style={CARD_HEAD}>{c.activityLabel}</span>
                        </span>
                        <span style={{ textAlign: "right" }}>
                          <span style={CARD_VP}>{c.vpLabel} VP</span>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </section>
        )}

        {value === "candidate" && candidate && (
          <section className="gv-delegate-vp__candidate" style={WRAP}>
            <h1 style={H1}>{candidate.name}</h1>
            <p style={SUB}>
              <code>{candidate.address}</code>
            </p>
            <p style={SUB}>
              {candidate.archiveVotes} vote
              {candidate.archiveVotes === 1 ? "" : "s"} in the last{" "}
              {data.rosterWindowDays} days &#xB7; {candidate.vpLabel} VP
            </p>
            {candidate.vpDistribution ? (
              <VpTable distribution={candidate.vpDistribution} />
            ) : (
              <p style={NOTE}>Live Voting Power for this address is unavailable.</p>
            )}
            <div style={CONTROLS}>
              <button type="button" style={BTN} onClick={() => send({ type: "BACK" })}>
                Back to delegates
              </button>
              <button
                type="button"
                style={BTN_PRIMARY}
                onClick={() => send({ type: "CONFIRM" })}
              >
                Delegate VP to {candidate.name}
              </button>
            </div>
          </section>
        )}

        {value === "confirming" && candidate && (
          <section className="gv-delegate-vp__confirm" style={WRAP}>
            <h1 style={H1}>Confirm delegation</h1>
            <p style={SUB}>
              This sends a <code>setDelegate</code> transaction from your wallet. It
              costs gas, and it stays in effect until you delegate elsewhere or clear
              it on-chain.
            </p>
            <dl style={CONFIRM_GRID}>
              <div style={CONFIRM_ROW}>
                <dt style={DT}>Delegate to</dt>
                <dd style={DD}>{candidate.address}</dd>
              </div>
              <div style={CONFIRM_ROW}>
                <dt style={DT}>Your Voting Power</dt>
                <dd style={DD}>{data.userVpLabel}</dd>
              </div>
              <div style={CONFIRM_ROW}>
                <dt style={DT}>Snapshot space</dt>
                <dd style={DD}>
                  <code>{data.space}</code>
                </dd>
              </div>
              <div style={CONFIRM_ROW}>
                <dt style={DT}>Registry</dt>
                <dd style={{ ...DD, fontFamily: "monospace", fontSize: 12 }}>
                  {data.registry
                    ? `${data.registry.address} \u{B7} chain ${data.registry.chainId}`
                    : "not configured"}
                </dd>
              </div>
            </dl>
            <Blockers items={data.blockers} />
            <div style={CONTROLS}>
              <button type="button" style={BTN} onClick={() => send({ type: "BACK" })}>
                Back
              </button>
              <button
                type="button"
                style={BTN_PRIMARY}
                onClick={() => send({ type: "SIGN" })}
              >
                Sign in wallet
              </button>
            </div>
          </section>
        )}

        {value === "signing" && (
          <section className="gv-delegate-vp__signing" style={WRAP} aria-busy="true">
            <h1 style={H1}>Waiting for your wallet&#x2026;</h1>
            <p style={SUB}>
              Confirm the <code>setDelegate</code> transaction in your wallet. This
              screen waits for the transaction receipt.
            </p>
            <div style={{ ...SPINNER }} aria-hidden="true" />
          </section>
        )}

        {value === "done" && candidate && receipt && (
          <section className="gv-delegate-vp__done" style={WRAP}>
            <h1 style={H1}>
              {receipt.status === "confirmed"
                ? "Delegation confirmed"
                : "Delegation submitted"}
            </h1>
            <p style={SUB}>
              {receipt.status === "confirmed" ? (
                <>
                  Your Voting Power in <code>{data.space}</code> is delegated to{" "}
                  <strong>{candidate.name}</strong>
                  {receipt.blockNumber !== null ? ` in block ${receipt.blockNumber}` : ""}.
                </>
              ) : (
                <>
                  The transaction was sent but has not been confirmed yet. It takes
                  effect once it is mined.
                </>
              )}
            </p>
            <dl style={CONFIRM_GRID}>
              <div style={CONFIRM_ROW}>
                <dt style={DT}>Transaction</dt>
                <dd style={{ ...DD, wordBreak: "break-all", fontFamily: "monospace", fontSize: 12 }}>
                  {receipt.txHash}
                </dd>
              </div>
              <div style={CONFIRM_ROW}>
                <dt style={DT}>Chain</dt>
                <dd style={DD}>{receipt.chainId}</dd>
              </div>
            </dl>
          </section>
        )}

        {value === "error" && (
          <section className="gv-delegate-vp__error" style={WRAP}>
            <h1 style={H1}>Couldn't delegate</h1>
            <p style={{ ...SUB, color: "#c0392b" }} role="alert">
              {state.context.error ?? "The delegate transaction failed."}
            </p>
            <div style={CONTROLS}>
              <button type="button" style={BTN} onClick={() => send({ type: "BACK" })}>
                Back to confirm
              </button>
              <button
                type="button"
                style={BTN_PRIMARY}
                onClick={() => send({ type: "RETRY" })}
              >
                Retry
              </button>
            </div>
          </section>
        )}
      </div>
    </GovernanceChrome>
  );
}

function VpTable({ distribution }: { distribution: VpDistribution }) {
  return (
    <dl style={CONFIRM_GRID}>
      {DISTRIBUTION.map(({ key, label }) => (
        <div key={key} style={CONFIRM_ROW}>
          <dt style={DT}>{label}</dt>
          <dd style={DD}>{vpLabel(distribution[key])}</dd>
        </div>
      ))}
    </dl>
  );
}

function Blockers({ items }: { items: string[] }) {
  if (items.length === 0) return null;
  return (
    <ul style={BLOCKERS} aria-label="Unavailable data">
      {items.map((item) => (
        <li key={item}>{item}</li>
      ))}
    </ul>
  );
}

const WRAP: React.CSSProperties = {
  maxWidth: 720,
  margin: "0 auto",
  padding: "32px 24px 64px",
  color: "#16141a",
};
const H1: React.CSSProperties = { fontSize: 26, fontWeight: 700, margin: "0 0 8px", color: "#fcfcfc" };
const SUB: React.CSSProperties = { fontSize: 15, lineHeight: 1.5, color: "#b4b0be", margin: "0 0 12px" };
const NOTE: React.CSSProperties = {
  fontSize: 13,
  color: "#cfcbd7",
  background: "rgba(255,45,85,.06)",
  border: "1px solid rgba(255,45,85,.18)",
  borderRadius: 10,
  padding: "10px 14px",
  margin: "16px 0",
};
const BLOCKERS: React.CSSProperties = {
  ...NOTE,
  listStyle: "disc",
  paddingLeft: 32,
  paddingRight: 14,
};
const EMPTY: React.CSSProperties = {
  border: "1px solid var(--gv-line, rgba(255,255,255,.1))",
  background: "var(--gv-surface, #18171c)",
  borderRadius: 14,
  padding: "28px 24px",
  fontSize: 14,
  lineHeight: 1.55,
  color: "#b4b0be",
};
const EMPTY_LINK: React.CSSProperties = {
  display: "inline-block",
  padding: "10px 18px",
  borderRadius: 10,
  border: "none",
  background: "var(--brand-cta)",
  color: "#fff",
  fontWeight: 600,
  textDecoration: "none",
};
const CARD_GRID: React.CSSProperties = { listStyle: "none", margin: 0, padding: 0, display: "grid", gap: 12 };
const CARD: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 14,
  width: "100%",
  padding: "14px 16px",
  borderRadius: 14,
  border: "1px solid var(--gv-line, rgba(255,255,255,.1))",
  background: "var(--gv-surface, #18171c)",
  cursor: "pointer",
};
const CARD_NAME: React.CSSProperties = { display: "block", fontWeight: 600, fontSize: 16, color: "var(--gv-ink, #fcfcfc)" };
const CARD_ADDR: React.CSSProperties = { display: "block", fontSize: 12, color: "var(--gv-muted, #b9b6c0)", marginBottom: 4 };
const CARD_HEAD: React.CSSProperties = { display: "block", fontSize: 13, color: "var(--gv-muted, #b9b6c0)" };
const CARD_VP: React.CSSProperties = { display: "block", fontWeight: 700, fontSize: 15, color: "var(--gv-ink, #fcfcfc)" };
const CONTROLS: React.CSSProperties = {
  display: "flex",
  gap: 12,
  justifyContent: "flex-end",
  maxWidth: 720,
  margin: "16px auto 0",
  padding: "0 24px",
};
const BTN: React.CSSProperties = {
  padding: "10px 18px",
  borderRadius: 10,
  border: "1px solid rgba(0,0,0,.18)",
  background: "#fff",
  color: "#16141a",
  fontWeight: 600,
  cursor: "pointer",
};
const BTN_PRIMARY: React.CSSProperties = {
  ...BTN,
  border: "none",
  background: "var(--brand-cta)",
  color: "#fff",
};
const CONFIRM_GRID: React.CSSProperties = {
  display: "grid",
  gap: 0,
  border: "1px solid rgba(255,255,255,.12)",
  borderRadius: 14,
  overflow: "hidden",
  margin: 0,
};
const CONFIRM_ROW: React.CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "center",
  padding: "14px 16px",
  borderBottom: "1px solid rgba(255,255,255,.08)",
};
const DT: React.CSSProperties = { margin: 0, fontSize: 13, color: "#b4b0be", fontWeight: 600 };
const DD: React.CSSProperties = { margin: 0, fontSize: 15, fontWeight: 600, textAlign: "right", color: "#fcfcfc" };
const SPINNER: React.CSSProperties = {
  width: 36,
  height: 36,
  borderRadius: "50%",
  border: "3px solid rgba(0,0,0,.12)",
  borderTopColor: "#ff2d55",
  animation: "u-spin 0.8s linear infinite",
};
