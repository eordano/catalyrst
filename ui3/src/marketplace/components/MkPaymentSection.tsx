import type { FormEvent, ReactNode } from "react";

import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import { creditsNoun } from "../credits-unit";
import "./paymentsection.css";

export type MkPayMethod = "card" | "mana";


function manaFor(wei: string): string {
  try {
    const hundredths = BigInt(wei) / 10_000_000_000_000_000n;
    return (Number(hundredths) / 100).toFixed(2);
  } catch {
    return "—";
  }
}

type MkPaymentSectionProps = {
  method?: MkPayMethod | null;
  shortfallCredits?: string;
  onPickMethod?: (method: MkPayMethod) => void;
  children?: ReactNode;
};

export default function MkPaymentSection({
  method = null,
  shortfallCredits = "",
  onPickMethod = undefined,
  children = undefined,
}: MkPaymentSectionProps) {
  return (
    <div className="paysec" data-pane={method ?? "none"}>
      <p className="paysec__need" role="status">
        You need <strong>{shortfallCredits} more {creditsNoun(shortfallCredits, true)}</strong>:
      </p>
      <div className="paysec__methods" role="group" aria-label="Payment method">
        <button
          type="button"
          className="paysec__method"
          aria-pressed={method === "card"}
          onClick={() => onPickMethod?.("card")}
        >
          <span className="paysec__methodicon" aria-hidden>
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
              <rect x="2.5" y="5" width="19" height="14" rx="2.5" />
              <path d="M2.5 9.5h19" />
            </svg>
          </span>
          Pay with card
        </button>
        <button
          type="button"
          className="paysec__method"
          aria-pressed={method === "mana"}
          onClick={() => onPickMethod?.("mana")}
        >
          <span className="paysec__methodicon" aria-hidden>
            ◇
          </span>
          Pay with MANA
        </button>
      </div>
      <div className="paysec__pane">{children}</div>
    </div>
  );
}

export type MkCardPhase = "idle" | "paying" | "done" | "off";

type MkPaymentCardPaneProps = {
  phase?: MkCardPhase;
  error?: string | null;
  granted?: string | null;
  onSubmit?: (e: FormEvent) => void;
};

export function MkPaymentCardPane({
  phase = "idle",
  error = null,
  granted = null,
  onSubmit = undefined,
}: MkPaymentCardPaneProps) {
  if (phase === "off") {
    return (
      <p className="paysec__unavailable" role="status">
        Card payments aren&apos;t available right now — pay with MANA, or{" "}
        <a className="mkco__link" href="/marketplace/packs">
          buy a Credits pack
        </a>
        .
      </p>
    );
  }

  if (phase === "done") {
    return (
      <p className="paysec__done" role="status">
        {granted} {creditsNoun(granted, true)} added — completing your checkout…
      </p>
    );
  }

  return (
    <form onSubmit={onSubmit} className="paysec__form">
      <div className="mkco__badge">MOCK payment · test card · no real charge</div>
      <label className="mkco__field">
        <span className="mkco__fieldlabel">Card number</span>
        <input
          className="mkco__fieldinput"
          inputMode="numeric"
          placeholder="4242 4242 4242 4242"
          defaultValue="4242 4242 4242 4242"
          aria-label="Card number (test)"
        />
      </label>
      <div className="mkco__fieldrow">
        <label className="mkco__field" style={{ flex: 1 }}>
          <span className="mkco__fieldlabel">Expiry</span>
          <input
            className="mkco__fieldinput"
            placeholder="12 / 34"
            defaultValue="12 / 34"
            aria-label="Expiry (test)"
          />
        </label>
        <label className="mkco__field" style={{ width: 110 }}>
          <span className="mkco__fieldlabel">CVC</span>
          <input
            className="mkco__fieldinput"
            placeholder="123"
            defaultValue="123"
            aria-label="CVC (test)"
          />
        </label>
      </div>
      {error && (
        <p role="alert" className="mkco__alert">
          {error}
        </p>
      )}
      <Button variant="primary" type="submit" disabled={phase === "paying"}>
        {phase === "paying" ? "Processing…" : "Continue"}
      </Button>
    </form>
  );
}

export type MkManaPhase =
  | { step: "loading" }
  | { step: "unavailable"; why: string }
  | { step: "ready"; quote: { weiSuggested: string } }
  | { step: "signing" }
  | { step: "confirming"; txHash: string }
  | { step: "done"; granted: string }
  | { step: "error"; message: string };

type MkPaymentManaPaneProps = {
  phase?: MkManaPhase;
  credits?: string;
  onPay?: () => void;
  onStartOver?: () => void;
};

export function MkPaymentManaPane({
  phase = { step: "loading" },
  credits = "",
  onPay = undefined,
  onStartOver = undefined,
}: MkPaymentManaPaneProps) {
  if (phase.step === "loading") {
    return (
      <p className="paysec__busy" role="status">
        <Spinner size={16} /> Getting a MANA quote…
      </p>
    );
  }

  if (phase.step === "unavailable") {
    return (
      <p className="paysec__unavailable" role="status">
        {phase.why}{" "}
        <a className="mkco__link" href="/marketplace/packs">
          Buy a Credits pack instead
        </a>
        .
      </p>
    );
  }

  if (phase.step === "signing") {
    return (
      <p className="paysec__busy" role="status">
        <Spinner size={16} /> Check your wallet — sign the MANA transfer.
      </p>
    );
  }

  if (phase.step === "confirming") {
    return (
      <p className="paysec__busy" role="status">
        <Spinner size={16} /> Confirming on Polygon… (
        <a
          href={`https://polygonscan.com/tx/${phase.txHash}`}
          target="_blank"
          rel="noopener noreferrer"
          className="mkco__link"
        >
          view transaction
        </a>
        )
      </p>
    );
  }

  if (phase.step === "done") {
    return (
      <p className="paysec__done" role="status">
        {phase.granted} {creditsNoun(phase.granted, true)} added from your MANA — completing your
        checkout…
      </p>
    );
  }

  if (phase.step === "error") {
    return (
      <div className="paysec__form">
        <p role="alert" className="mkco__alert">
          {phase.message}
        </p>
        <Button variant="secondary" onClick={onStartOver}>
          Start over
        </Button>
      </div>
    );
  }

  return (
    <div className="paysec__form">
      <p className="paysec__quote">
        Send <strong>≈{manaFor(phase.quote.weiSuggested)} MANA</strong> for{" "}
        <strong>{credits} {creditsNoun(credits, true)}</strong>.
      </p>
      <Button variant="primary" onClick={onPay}>
        Continue
      </Button>
      <p className="mkco__note">Gas is on us. Any surplus stays as Credits.</p>
    </div>
  );
}
