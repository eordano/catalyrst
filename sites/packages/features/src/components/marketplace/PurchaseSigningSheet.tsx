import { useMemo } from "react";

import Button from "@ui/atoms/Button";
import Spinner from "@ui/atoms/Spinner";
import {
  MkCheckoutActions,
  MkCheckoutSummary,
} from "@ui/marketplace/pages/MkCheckout";

import {
  purchaseIntentTypedData,
  type PurchaseIntent,
} from "@data/lib/catalyst/marketplace/purchase-intent";

export type SigningSheetLine = {
  key: string;
  name: string;
  qty: number;
  unitPriceCredits: string;
};

export default function PurchaseSigningSheet({
  intent,
  lines,
  signing,
  error,
  notice,
  onSign,
  onCancel,
}: {
  intent: PurchaseIntent;
  lines: SigningSheetLine[];
  signing: boolean;
  error: string | null;
  notice?: string | null;
  onSign: () => void;
  onCancel: () => void;
}) {
  const typedData = useMemo(() => purchaseIntentTypedData(intent), [intent]);
  const expires = new Date(intent.expiresAt * 1000);

  return (
    <div
      className={"mkco__card" + (notice ? " mkco__card--info" : "")}
      role="dialog"
      aria-modal="true"
      aria-label="Review and sign your purchase"
    >
      {notice && (
        <p role="alert" className="mkco__notice" data-testid="signing-sheet-price-changed">
          <strong>Price changed &#x2014;</strong> {notice} Review the new numbers and
          sign again; your previous signature was discarded and can't be
          charged.
        </p>
      )}
      <p className="mkco__lead">Review &amp; sign this purchase</p>
      <p className="mkco__muted">
        Your wallet will sign a purchase authorization for exactly the
        following &#x2014; nothing else:
      </p>

      <MkCheckoutSummary
        heading="You're signing for"
        lines={lines.map((l) => ({
          key: l.key,
          name: l.name,
          qty: l.qty,
          unitPriceCredits: l.unitPriceCredits,
        }))}
      />

      <dl className="mkco__meta">
        <dt className="mkco__metakey">Total</dt>
        <dd className={"mkco__metaval" + (notice ? " mkco__metaval--new" : "")}>
          <strong>
            {intent.totalCredits} {intent.currency}
          </strong>
          {notice && <span className="mkco__newtag">new price</span>}
        </dd>
        <dt className="mkco__metakey">Buyer wallet</dt>
        <dd className="mkco__metaval mkco__metaval--mono">{intent.buyer}</dd>
        <dt className="mkco__metakey">Valid until</dt>
        <dd className="mkco__metaval">{expires.toLocaleString()}</dd>
        <dt className="mkco__metakey">Order reference</dt>
        <dd className="mkco__metaval mkco__metaval--mono">{intent.nonce}</dd>
      </dl>

      <details className="mkco__rawwrap">
        <summary className="mkco__rawsummary">
          Show the raw payload your wallet signs (EIP-712)
        </summary>
        <pre className="mkco__raw" data-testid="signing-sheet-raw-payload">
          {JSON.stringify(typedData, null, 2)}
        </pre>
      </details>

      {error && (
        <p role="alert" className="mkco__alert">
          {error}
        </p>
      )}

      <MkCheckoutActions>
        <Button variant="primary" disabled={signing} onClick={onSign}>
          {signing ? (
            <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
              <Spinner size={16} /> Waiting for your wallet&#x2026;
            </span>
          ) : (
            "Sign & confirm purchase"
          )}
        </Button>
        <Button variant="secondary" disabled={signing} onClick={onCancel}>
          Go back
        </Button>
      </MkCheckoutActions>
    </div>
  );
}
