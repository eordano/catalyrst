import { useEffect, useRef, useState } from "react";

import MkPaymentSection, {
  MkPaymentCardPane,
  MkPaymentManaPane,
  type MkPayMethod,
} from "@ui/marketplace/components/MkPaymentSection";

import type { AuthIdentity } from "@data/lib/auth/index";
import { signTypedData } from "@data/lib/auth/typed-data";
import {
  executeMetaTxCalldata,
  manaMetaTxTypedData,
  relayMetaTx,
  transferCalldata,
} from "@data/lib/catalyst/marketplace/mana-pay";
import {
  clearPendingTopup,
  getPendingTopup,
  setPendingTopup,
} from "@data/lib/catalyst/marketplace/pending-topup";
import {
  fetchManaNonce,
  fetchPaymentsConfig,
  isMockCardOff,
  mockCardTopup,
  quoteManaTopup,
  redeemManaTopup,
  type ManaTopupQuote,
  type PaymentsConfig,
} from "@data/lib/catalyst/marketplace/topup";
import { track } from "@core/lib/telemetry/track";
import type { TrackContext } from "@core/lib/telemetry/track";

import {
  MANA_UNAVAILABLE,
  defaultPayMethod,
  manaIdlePhase,
  manaQuoteState,
  type ManaBalance,
  type ManaIdlePhase,
  type ManaQuoteState,
} from "./mana-phase";
import { useManaBalance } from "./use-mana-balance";

const REDEEM_POLL_MS = 3000;
const REDEEM_POLL_MAX = 60;

type PayMethod = MkPayMethod;

export default function PaymentSection({
  identity,
  shortfallCredits,
  preselect,
  trackCtx,
  onToppedUp,
}: {
  identity: AuthIdentity;
  shortfallCredits: string;
  preselect?: PayMethod;
  trackCtx: TrackContext;
  onToppedUp: () => void | Promise<void>;
}) {
  const [method, setMethod] = useState<PayMethod | null>(preselect ?? null);
  const chosen = useRef(preselect != null);
  const { balance, refresh: refreshBalance } = useManaBalance(identity.signer);

  useEffect(() => {
    if (chosen.current || balance === undefined) return;
    chosen.current = true;
    setMethod(defaultPayMethod(balance));
  }, [balance]);

  function pick(m: PayMethod) {
    chosen.current = true;
    if (m === method) return;
    setMethod(m);
    track("mk_pay_method_selected", { method: m }, trackCtx);
  }

  return (
    <MkPaymentSection
      method={method}
      shortfallCredits={shortfallCredits}
      onPickMethod={pick}
    >
      {method === "card" && (
        <CardPane
          identity={identity}
          credits={shortfallCredits}
          trackCtx={trackCtx}
          onToppedUp={onToppedUp}
        />
      )}
      {method === "mana" && (
        <ManaPane
          identity={identity}
          credits={shortfallCredits}
          balance={balance}
          onRefreshBalance={refreshBalance}
          trackCtx={trackCtx}
          onToppedUp={onToppedUp}
          onPayWithCard={() => pick("card")}
        />
      )}
    </MkPaymentSection>
  );
}

function CardPane({
  identity,
  credits,
  trackCtx,
  onToppedUp,
}: {
  identity: AuthIdentity;
  credits: string;
  trackCtx: TrackContext;
  onToppedUp: () => void | Promise<void>;
}) {
  const [phase, setPhase] = useState<"idle" | "paying" | "done" | "off">("idle");
  const [error, setError] = useState<string | null>(null);
  const [granted, setGranted] = useState<string | null>(null);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (phase === "paying" || phase === "done") return;
    setPhase("paying");
    setError(null);
    try {
      const res = await mockCardTopup(identity, credits);
      setGranted(res.creditsGranted);
      setPhase("done");
      track(
        "mk_card_topup",
        { credits: res.creditsGranted, mock: true },
        trackCtx,
      );
      await onToppedUp();
    } catch (err) {
      if (isMockCardOff(err)) {
        setPhase("off");
        return;
      }
      setPhase("idle");
      setError((err as Error)?.message ?? "The card payment failed.");
    }
  }

  return (
    <MkPaymentCardPane
      phase={phase}
      error={error}
      granted={granted}
      onSubmit={onSubmit}
    />
  );
}

type ManaActionPhase =
  | { step: "signing"; quote: ManaTopupQuote }
  | { step: "confirming"; txHash: string }
  | { step: "done"; granted: string; txHash: string }
  | { step: "error"; message: string };

type ManaPhase = ManaIdlePhase | ManaActionPhase;

type ManaPaneProps = {
  identity: AuthIdentity;
  credits: string;
  balance: ManaBalance;
  onRefreshBalance: () => void;
  trackCtx: TrackContext;
  onToppedUp: () => void | Promise<void>;
  onPayWithCard?: () => void;
};

export function ManaTopupPane(
  props: Omit<ManaPaneProps, "balance" | "onRefreshBalance">,
) {
  const { balance, refresh } = useManaBalance(props.identity.signer);
  return <ManaPane {...props} balance={balance} onRefreshBalance={refresh} />;
}

function ManaPane({
  identity,
  credits,
  balance,
  onRefreshBalance,
  trackCtx,
  onToppedUp,
  onPayWithCard,
}: ManaPaneProps) {
  const [quoted, setQuoted] = useState<ManaQuoteState>({ step: "loading" });
  const [action, setAction] = useState<ManaActionPhase | null>(null);
  const [attempt, setAttempt] = useState(0);
  const phase: ManaPhase = action ?? manaIdlePhase(quoted, balance);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const pending = getPendingTopup(identity.signer);
    if (pending) {
      void confirmAndRedeem(pending.txHash);
      return () => {
        cancelled = true;
      };
    }
    setQuoted({ step: "loading" });
    Promise.all([fetchPaymentsConfig(), quoteManaTopup(credits)])
      .then(([config, quote]) => {
        if (!cancelled) setQuoted(manaQuoteState(config, quote));
      })
      .catch(() => {
        if (!cancelled) setQuoted({ step: "unavailable", why: MANA_UNAVAILABLE });
      });
    return () => {
      cancelled = true;
    };
  }, [credits, attempt]);

  async function confirmAndRedeem(txHash: string) {
    setAction({ step: "confirming", txHash });
    for (let i = 0; i < REDEEM_POLL_MAX; i++) {
      const res = await redeemManaTopup(identity, txHash);
      if (!alive.current) return;
      if (res.state === "granted") {
        clearPendingTopup(identity.signer);
        setAction({ step: "done", granted: res.creditsGranted, txHash });
        track(
          "mk_mana_topup_granted",
          { tx: txHash, credits: res.creditsGranted },
          trackCtx,
        );
        await onToppedUp();
        return;
      }
      await new Promise((r) => setTimeout(r, REDEEM_POLL_MS));
    }
    setAction({
      step: "error",
      message:
        "The transfer is taking longer than expected. Your MANA is safe and your receipt is saved \u{2014} come back to checkout any time and the Credits will finish arriving.",
    });
  }

  async function pay(quote: ManaTopupQuote, config: PaymentsConfig) {
    const from = identity.signer.toLowerCase();
    setAction({ step: "signing", quote });
    try {
      const nonce = await fetchManaNonce(from);
      const fn = transferCalldata(config.payTo as string, quote.weiSuggested);
      const typed = manaMetaTxTypedData(from, nonce, fn);
      const signature = await signTypedData(typed, from);
      const txData = executeMetaTxCalldata(from, signature, fn);
      const txHash = await relayMetaTx(from, txData);
      setPendingTopup(identity.signer, { txHash, ts: Date.now() });
      track("mk_mana_topup_relayed", { tx: txHash }, trackCtx);
      if (!alive.current) return;
      await confirmAndRedeem(txHash);
    } catch (err) {
      if (!alive.current) return;
      setAction({
        step: "error",
        message:
          (err as Error)?.message ??
          "The MANA payment couldn't be completed. Nothing left your wallet unless the transfer confirmed.",
      });
    }
  }

  return (
    <MkPaymentManaPane
      phase={phase}
      credits={credits}
      onPay={() => {
        if (phase.step === "ready") void pay(phase.quote, phase.config);
      }}
      onStartOver={() => {
        setAction(null);
        setAttempt((n) => n + 1);
        onRefreshBalance();
      }}
      onPayWithCard={onPayWithCard}
    />
  );
}
