import {
  MkCheckoutFrame,
  MkCheckoutCard,
} from "@ui/marketplace/pages/MkCheckout";

import type { DisplayLine } from "@data/lib/catalyst/marketplace/cart-display";
import { newIdempotencyKey } from "@data/lib/catalyst/marketplace/checkout";
import type { CancelTarget } from "@data/lib/catalyst/marketplace/checkout-run";
import { buildPurchaseIntent } from "@data/lib/catalyst/marketplace/purchase-intent";
import {
  FailedCard,
  PriceDriftCard,
  ProcessingCard,
  ResumeCheckingCard,
  ResumeDoneCard,
} from "./CheckoutCards";
import PurchaseSigningSheet from "./PurchaseSigningSheet";

const DEV_PREVIEW_LINES: DisplayLine[] = [
  { key: "a", name: "ENO T-Shirt", qty: 1, unitPriceCredits: "1" },
  { key: "b", name: "Golfcraft - Eating Paella", qty: 1, unitPriceCredits: "1" },
];
const DEV_PREVIEW_CANCEL: CancelTarget = { label: "Back to cart", href: "/marketplace/cart" };

export default function DevStatePreview({ state, tour }: { state: string; tour?: boolean }) {
  const noop = () => {};
  const intent = buildPurchaseIntent({
    buyer: "0x951bb66ce4a5d4b1c667e386af5313753d14ba2e",
    lines: [
      { collection: "0xc78d22b25514ebef5cdb34ccc4bf85f28c30f8c4", itemId: "0", qty: 1 },
      { collection: "0xd14026bf5e455f3487dc57cbff3d408d4c11949f", itemId: "0", qty: 1 },
    ],
    totalCredits: "2",
    nonce: newIdempotencyKey(),
  });
  const sheetLines = DEV_PREVIEW_LINES.map((l) => ({
    key: l.key,
    name: l.name,
    qty: l.qty,
    unitPriceCredits: l.unitPriceCredits,
  }));

  const states: Record<string, React.ReactNode> = {
    drift: (
      <PriceDriftCard
        message="The price of this order changed after you signed."
        cancel={DEV_PREVIEW_CANCEL}
        onReview={noop}
      />
    ),
    "drift-sheet": (
      <PurchaseSigningSheet
        intent={intent}
        lines={sheetLines}
        signing={false}
        error={null}
        notice="The price of this order changed after you signed."
        onSign={noop}
        onCancel={noop}
      />
    ),
    sheet: (
      <PurchaseSigningSheet
        intent={intent}
        lines={sheetLines}
        signing={false}
        error={null}
        notice={null}
        onSign={noop}
        onCancel={noop}
      />
    ),
    "partial-failure": (
      <FailedCard
        status="failed"
        checkoutId={7777}
        cancel={DEV_PREVIEW_CANCEL}
        onRetry={noop}
      />
    ),
    reversed: (
      <FailedCard
        status="reversed"
        checkoutId={7777}
        cancel={DEV_PREVIEW_CANCEL}
        onRetry={noop}
      />
    ),
    "sold-out": (
      <FailedCard
        error={"no open marketplace listing to fulfil this item from \u{2014} it may have just been bought"}
        cancel={DEV_PREVIEW_CANCEL}
        onRetry={noop}
      />
    ),
    insufficient: (
      <FailedCard
        error="insufficient credits"
        cancel={DEV_PREVIEW_CANCEL}
        onRetry={noop}
      />
    ),
    processing: <ProcessingCard items="items" checkoutId={7777} />,
    "resume-checking": <ResumeCheckingCard checkoutId={7777} />,
    "resume-done": <ResumeDoneCard />,
  };

  const body = states[state];
  return (
    <MkCheckoutFrame back={DEV_PREVIEW_CANCEL && { href: DEV_PREVIEW_CANCEL.href, label: DEV_PREVIEW_CANCEL.label }}>
      {tour ? null : (
        <p
          role="note"
          style={{
            margin: "0 0 12px",
            padding: "6px 12px",
            borderRadius: 8,
            border: "1px dashed rgba(255, 209, 102, 0.6)",
            color: "#ffd166",
            fontSize: 12,
            letterSpacing: 0.4,
          }}
        >
          DEV STATE PREVIEW &#x2014; ?state={state} &#xB7; canned data, nothing real &#xB7;
          available: {Object.keys(states).join(" \u{B7} ")}
        </p>
      )}
      {body ?? (
        <MkCheckoutCard>
          <p className="mkco__lead">Unknown state &#x201C;{state}&#x201D;</p>
          <p className="mkco__muted">Pick one from the banner above.</p>
        </MkCheckoutCard>
      )}
    </MkCheckoutFrame>
  );
}
