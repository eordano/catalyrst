import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import Button from "@ui/atoms/Button";
import Spinner from "@ui/atoms/Spinner";
import {
  MkCheckoutFrame,
  MkCheckoutCard,
  MkCheckoutActions,
  MkCheckoutSummary,
} from "@ui/marketplace/pages/MkCheckout";
import { creditsNoun } from "@ui/marketplace/credits-unit";

import type { AuthIdentity } from "@data/lib/auth/index";
import type { DisplayLine } from "@data/lib/catalyst/marketplace/cart-display";
import {
  checkoutErrorMessage,
  isPriceDriftError,
  newIdempotencyKey,
  type Balance,
} from "@data/lib/catalyst/marketplace/checkout";
import type {
  CancelTarget,
  CreateRun,
  FreshLines,
  RefreshLines,
} from "@data/lib/catalyst/marketplace/checkout-run";
import {
  buildPurchaseIntent,
  signPurchaseIntent,
  type IntentLine,
  type PurchaseIntent,
  type SignedPurchaseIntent,
} from "@data/lib/catalyst/marketplace/purchase-intent";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  checkoutMachine,
  stateToSlug,
  type FulfillFn,
} from "../../stories/marketplace/checkout/machine";
import { FailedCard, PriceDriftCard, ProcessingCard } from "./CheckoutCards";
import PaymentSection from "./PaymentSection";
import PurchaseSigningSheet from "./PurchaseSigningSheet";

export default function CheckoutWizard({
  createRun,
  identity,
  buyer,
  intentLines,
  totalCredits,
  itemCount,
  balance,
  refreshBalance,
  payPreselect,
  trackCtx,
  cancel,
  lines,
  refreshLines,
  onSpendCommitted,
}: {
  createRun: CreateRun;
  identity: AuthIdentity;
  buyer: string;
  intentLines: IntentLine[];
  totalCredits: string;
  itemCount: number;
  balance: Balance | null;
  refreshBalance: () => Promise<void>;
  payPreselect?: "card" | "mana";
  trackCtx: TrackContext;
  cancel: CancelTarget;
  lines?: DisplayLine[];
  refreshLines?: RefreshLines;
  onSpendCommitted?: () => void;
}) {
  const idemRef = useRef(newIdempotencyKey());

  const signedRef = useRef<SignedPurchaseIntent | null>(null);

  const [drift, setDrift] = useState<{ message: string; fresh: FreshLines } | null>(
    null,
  );
  const refreshRef = useRef<RefreshLines | undefined>(refreshLines);
  refreshRef.current = refreshLines;

  const run = useMemo<FulfillFn>(() => {
    const inner = createRun(() => signedRef.current);
    return async (args) => {
      try {
        return await inner({ ...args, idempotencyKey: idemRef.current });
      } catch (err) {
        if (isPriceDriftError(err)) {
          signedRef.current = null;
          idemRef.current = newIdempotencyKey();
          const fresh = await (refreshRef.current?.() ?? Promise.resolve(null)).catch(
            () => null,
          );
          const message =
            checkoutErrorMessage(err) ??
            "The price of this order changed after you signed.";
          setDrift(fresh ? { message, fresh } : null);
        }
        throw err;
      }
    };
  }, [createRun]);
  const [pendingIntent, setPendingIntent] = useState<PurchaseIntent | null>(null);
  const [signing, setSigning] = useState(false);
  const [signError, setSignError] = useState<string | null>(null);

  const [state, send] = useMachine(checkoutMachine, {
    input: { totalCredits, idempotencyKey: idemRef.current, trackCtx, run },
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctx = state.context;

  const openDriftSheet = useCallback(
    (fresh: FreshLines) => {
      setSignError(null);
      setPendingIntent(
        buildPurchaseIntent({
          buyer,
          lines: fresh.intentLines,
          totalCredits: fresh.totalCredits,
          nonce: idemRef.current,
        }),
      );
    },
    [buyer],
  );

  useEffect(() => {
    if (value === "failed" && drift) openDriftSheet(drift.fresh);
  }, [value, drift, openDriftSheet]);

  const signAndConfirm = async () => {
    if (!pendingIntent || signing) return;
    setSigning(true);
    setSignError(null);
    try {
      signedRef.current = await signPurchaseIntent(pendingIntent);
      setPendingIntent(null);
      onSpendCommitted?.();
      send({ type: value === "failed" ? "RETRY" : "CONFIRM" });
      setDrift(null);
    } catch (err) {
      setSignError(
        (err as Error)?.message ??
          "Your wallet couldn't sign this purchase. Nothing was charged.",
      );
    } finally {
      setSigning(false);
    }
  };

  const totalNum = Number(totalCredits);
  const balanceNum = balance ? Number(balance.available) : null;
  const insufficient =
    balanceNum !== null &&
    Number.isFinite(balanceNum) &&
    Number.isFinite(totalNum) &&
    balanceNum < totalNum;
  const balanceKnown = balanceNum !== null && Number.isFinite(balanceNum);
  const shortfall =
    insufficient && balanceNum !== null
      ? String(Math.max(1, Math.ceil(totalNum - balanceNum)))
      : null;

  const [authorizing, setAuthorizing] = useState(false);

  const signsInvisibly = useCallback(async () => {
    const { getThirdwebSession } = await import("@data/lib/auth/thirdweb/index");
    const tw = getThirdwebSession();
    if (tw && tw.address.toLowerCase() === buyer.toLowerCase()) return true;
    const { hasDevSigner } = await import("@data/lib/auth/dev-identity");
    return hasDevSigner();
  }, [buyer]);

  const startPurchase = useCallback(
    async (opts: { skipSheet?: boolean } = {}) => {
      const intent = buildPurchaseIntent({
        buyer,
        lines: intentLines,
        totalCredits,
        nonce: idemRef.current,
      });
      setSignError(null);
      const silent = opts.skipSheet || (await signsInvisibly());
      if (!silent) {
        setPendingIntent(intent);
        return;
      }
      setAuthorizing(true);
      try {
        signedRef.current = await signPurchaseIntent(intent);
        onSpendCommitted?.();
        send({ type: value === "failed" ? "RETRY" : "CONFIRM" });
      } catch (err) {
        setSignError(
          (err as Error)?.message ??
            "Your wallet couldn't sign this purchase. Nothing was charged.",
        );
      } finally {
        setAuthorizing(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [buyer, intentLines, totalCredits, signsInvisibly, onSpendCommitted, value],
  );

  const toppedUp = useRef(false);
  useEffect(() => {
    if (
      toppedUp.current &&
      value === "review" &&
      !pendingIntent &&
      balanceKnown &&
      !insufficient
    ) {
      toppedUp.current = false;
      void startPurchase({ skipSheet: true });
    }
  }, [value, pendingIntent, balanceKnown, insufficient, startPurchase]);

  const items = itemCount === 1 ? "item" : "items";

  return (
    <MkCheckoutFrame
      back={{ href: cancel.href, label: cancel.label }}
      wide={value === "review" && !pendingIntent}
    >
      <div className="mk-checkout-wizard" data-step={step}>

      {value === "review" && pendingIntent && (
        <PurchaseSigningSheet
          intent={pendingIntent}
          lines={(lines ?? []).map((l) => ({
            key: l.key,
            name: l.name,
            qty: l.qty,
            unitPriceCredits: l.unitPriceCredits,
          }))}
          signing={signing}
          error={signError}
          notice={null}
          onSign={() => void signAndConfirm()}
          onCancel={() => {
            setPendingIntent(null);
            setSignError(null);
          }}
        />
      )}

      {value === "review" && !pendingIntent && (
        <MkCheckoutCard>
          <div className={insufficient && shortfall ? "mkco__split" : undefined}>
            <div>
              <p className="mkco__lead">
                Spend {totalCredits} {creditsNoun(totalCredits, true)} on {itemCount} {items}?
              </p>
              {balance ? (
                <p className="mkco__muted">
                  Your balance: {balance.available} {creditsNoun(balance.available, true)}
                </p>
              ) : (
                <p className="mkco__muted" aria-busy="true">
                  Checking your Credits balance&#x2026;
                </p>
              )}
              <MkCheckoutSummary
                heading="You're buying"
                lines={lines ?? []}
                total={totalCredits}
              />
              {!(insufficient && shortfall) && (
                <>
                  {signError && (
                    <p role="alert" className="mkco__alert">
                      {signError}
                    </p>
                  )}
                  <MkCheckoutActions>
                    <Button
                      variant="primary"
                      disabled={!balanceKnown || authorizing}
                      onClick={() => void startPurchase()}
                    >
                      {authorizing
                        ? "Authorizing\u{2026}"
                        : balanceKnown
                          ? "Continue"
                          : "Checking balance\u{2026}"}
                    </Button>
                  </MkCheckoutActions>
                </>
              )}
            </div>
            {insufficient && shortfall && (
              <div>
                {signError && (
                  <p role="alert" className="mkco__alert">
                    {signError}
                  </p>
                )}
                <PaymentSection
                  identity={identity}
                  shortfallCredits={shortfall}
                  preselect={payPreselect}
                  trackCtx={trackCtx}
                  onToppedUp={async () => {
                    toppedUp.current = true;
                    await refreshBalance();
                  }}
                />
              </div>
            )}
          </div>
        </MkCheckoutCard>
      )}

      {value === "fulfilling" && (
        <MkCheckoutCard busy>
          <div className="mkco__busy">
            <Spinner size={28} />
            <p className="mkco__lead">
              Processing your purchase &#x2014; please don't close this window.
            </p>
          </div>
          <p className="mkco__muted">
            We're debiting your Credits and delivering your {items}. This can take a
            moment.
          </p>
        </MkCheckoutCard>
      )}

      {value === "done" && (
        <MkCheckoutCard tone="success">
          <p className="mkco__lead mkco__lead--success">Purchase complete!</p>
          <p className="mkco__text">
            Your {items} {itemCount === 1 ? "is" : "are"} on the way to your
            account.
          </p>
          <MkCheckoutSummary heading="You bought" lines={lines ?? []} />
          <MkCheckoutActions>
            <a href="https://catalyst.example.com/play/" className="btn btn--primary btn--md">
              Jump in world &#x2192;
            </a>
            <Link to={href("/marketplace/account")} className="btn btn--secondary btn--md">
              View your items
            </Link>
            <Link to={href("/shop")} className="btn btn--secondary btn--md">
              Continue shopping
            </Link>
          </MkCheckoutActions>
        </MkCheckoutCard>
      )}

      {value === "processing" && (
        <ProcessingCard items={items} checkoutId={ctx.result?.checkoutId} />
      )}

      {value === "failed" && drift && pendingIntent && (
        <PurchaseSigningSheet
          intent={pendingIntent}
          lines={(lines ?? []).map((l) => ({
            key: l.key,
            name: l.name,
            qty: l.qty,
            unitPriceCredits: l.unitPriceCredits,
          }))}
          signing={signing}
          error={signError}
          notice={drift.message}
          onSign={() => void signAndConfirm()}
          onCancel={() => {
            setPendingIntent(null);
            setSignError(null);
          }}
        />
      )}

      {value === "failed" && drift && !pendingIntent && (
        <PriceDriftCard
          message={drift.message}
          cancel={cancel}
          onReview={() => openDriftSheet(drift.fresh)}
        />
      )}

      {value === "failed" && !drift && pendingIntent && (
        <PurchaseSigningSheet
          intent={pendingIntent}
          lines={(lines ?? []).map((l) => ({
            key: l.key,
            name: l.name,
            qty: l.qty,
            unitPriceCredits: l.unitPriceCredits,
          }))}
          signing={signing}
          error={signError}
          notice={null}
          onSign={() => void signAndConfirm()}
          onCancel={() => {
            setPendingIntent(null);
            setSignError(null);
          }}
        />
      )}

      {value === "failed" && !drift && !pendingIntent && (
        <FailedCard
          error={ctx.error}
          status={ctx.result?.status}
          checkoutId={ctx.result?.checkoutId}
          cancel={cancel}
          onRetry={() => {
            if (!signedRef.current) {
              void startPurchase();
              return;
            }
            send({ type: "RETRY" });
          }}
        />
      )}
      </div>
    </MkCheckoutFrame>
  );
}
