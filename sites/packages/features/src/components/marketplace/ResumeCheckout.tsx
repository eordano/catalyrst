import { useEffect, useState } from "react";

import Button from "@ui/atoms/Button";
import { MkCheckoutActions } from "@ui/marketplace/pages/MkCheckout";

import type { AuthIdentity } from "@data/lib/auth/index";
import {
  checkoutPhase,
  fetchCheckout,
  type CheckoutPhase,
} from "@data/lib/catalyst/marketplace/checkout";
import {
  delay,
  POLL_MAX,
  POLL_MS,
  type CancelTarget,
} from "@data/lib/catalyst/marketplace/checkout-run";
import { clearPendingCheckout } from "@data/lib/catalyst/marketplace/pending-checkout";
import {
  FailedCard,
  ProcessingCard,
  ResumeCheckingCard,
  ResumeDoneCard,
} from "./CheckoutCards";

const RESUME_CANCEL: CancelTarget = { label: "Continue shopping", href: "/shop" };

type ResumeState = { phase: CheckoutPhase | "checking"; status?: string };

function useResumeCheckout(identity: AuthIdentity, id: number): ResumeState {
  const [state, setState] = useState<ResumeState>({ phase: "checking" });

  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();

    (async () => {
      for (let i = 0; i < POLL_MAX; i++) {
        try {
          const c = await fetchCheckout(identity, id, controller.signal);
          const phase = checkoutPhase(c.status);
          if (cancelled) return;
          setState({ phase, status: c.status });
          if (phase !== "pending") {
            clearPendingCheckout(identity.signer);
            return;
          }
        } catch {
          if (controller.signal.aborted) return;
        }
        try {
          await delay(POLL_MS, controller.signal);
        } catch {
          return;
        }
      }
      if (!cancelled) {
        setState((s) => (s.phase === "checking" ? { phase: "pending" } : s));
      }
    })();

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [identity, id]);

  return state;
}

export default function ResumeCheckout({
  identity,
  checkoutId,
  onDismiss,
}: {
  identity: AuthIdentity;
  checkoutId: number;
  onDismiss: () => void;
}) {
  const { phase, status } = useResumeCheckout(identity, checkoutId);

  return (
    <>
      {phase === "checking" && <ResumeCheckingCard checkoutId={checkoutId} />}

      {phase === "pending" && (
        <>
          <ProcessingCard items="items" checkoutId={checkoutId} />
          <MkCheckoutActions>
            <Button variant="secondary" onClick={onDismiss}>
              Start a new checkout
            </Button>
          </MkCheckoutActions>
        </>
      )}

      {phase === "done" && <ResumeDoneCard />}

      {phase === "failed" && (
        <FailedCard
          status={status}
          checkoutId={checkoutId}
          cancel={RESUME_CANCEL}
          onRetry={() => {}}
        />
      )}
    </>
  );
}
