import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import Button from "@ui/atoms/Button";
import Spinner from "@ui/atoms/Spinner";
import {
  MkCheckoutFrame,
  MkCheckoutCard,
  MkCheckoutActions,
} from "@ui/marketplace/pages/MkCheckout";

import type { CancelTarget } from "@data/lib/catalyst/marketplace/checkout-run";

const LINK_BTN: React.CSSProperties = { textDecoration: "none" };

export function LoadingScreen({ label }: { label: string }) {
  return (
    <MkCheckoutFrame>
      <MkCheckoutCard busy>
        <div className="mkco__busy">
          <Spinner size={24} />
          <p className="mkco__lead">{label}</p>
        </div>
      </MkCheckoutCard>
    </MkCheckoutFrame>
  );
}

export function PriceDriftCard({
  message,
  cancel,
  onReview,
}: {
  message: string;
  cancel: CancelTarget;
  onReview: () => void;
}) {
  return (
    <MkCheckoutCard tone="info">
      <p role="alert" className="mkco__lead" style={{ color: "#ffd88a" }}>
        Price changed
      </p>
      <p className="mkco__text">{message}</p>
      <p className="mkco__muted">
        Nothing was charged, and your previous signature was discarded. Review
        the new price and sign again to continue.
      </p>
      <MkCheckoutActions>
        <Button variant="primary" onClick={onReview}>
          Review new price &amp; sign
        </Button>
        <Link to={cancel.href} className="mkco__link">
          {cancel.label}
        </Link>
      </MkCheckoutActions>
    </MkCheckoutCard>
  );
}

export function ResumeCheckingCard({ checkoutId }: { checkoutId: number }) {
  return (
    <MkCheckoutCard busy>
      <div className="mkco__busy">
        <Spinner size={28} />
        <p className="mkco__lead">You have a purchase in progress</p>
      </div>
      <p className="mkco__muted">Checking on checkout #{checkoutId}&#x2026;</p>
    </MkCheckoutCard>
  );
}

export function ResumeDoneCard() {
  return (
    <MkCheckoutCard tone="success">
      <p className="mkco__lead mkco__lead--success">Purchase complete!</p>
      <p className="mkco__text">Your items are on the way to your account.</p>
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
  );
}

export function ProcessingCard({
  items,
  checkoutId,
}: {
  items: string;
  checkoutId?: number;
}) {
  return (
    <MkCheckoutCard busy>
      <div className="mkco__busy">
        <Spinner size={28} />
        <p className="mkco__lead">Your purchase is still processing</p>
      </div>
      <p className="mkco__text">
        This can take a few minutes &#x2014; you don't need to buy again. If any item
        can't be delivered, its Credits are refunded to your balance
        automatically. Check &#x201C;View your items&#x201D;, or come back a little later.
      </p>
      {checkoutId != null && (
        <p className="mkco__muted">Checkout #{checkoutId}</p>
      )}
      <MkCheckoutActions>
        <Link to={href("/marketplace/account")} className="btn btn--primary btn--md">
          View your items
        </Link>
        <Link to={href("/shop")} className="btn btn--secondary btn--md">
          Continue shopping
        </Link>
      </MkCheckoutActions>
    </MkCheckoutCard>
  );
}

export function FailedCard({
  error,
  status,
  checkoutId,
  cancel,
  onRetry,
}: {
  error?: string;
  status?: string;
  checkoutId?: number;
  cancel: CancelTarget;
  onRetry: () => void;
}) {
  const reversed = !!status && /revers|refund/i.test(status);
  const insufficient = !!error && /insufficient/i.test(error);

  let title: string;
  let detail: string;
  let refundNote: string | null = null;
  let actions: React.ReactNode;

  if (status) {
    if (reversed) {
      title = "Purchase reversed";
      detail = "We couldn't deliver your items, so the order was rolled back.";
      refundNote = "Your Credits were fully refunded \u{2014} you were not charged.";
      actions = (
        <>
          <Link
            to={cancel.href}
            className="btn btn--primary btn--md"
            style={LINK_BTN}
          >
            {cancel.label}
          </Link>
          <Link
            to={href("/shop")}
            className="btn btn--secondary btn--md"
            style={LINK_BTN}
          >
            Back to marketplace
          </Link>
        </>
      );
    } else {
      title = "Order partly completed";
      detail =
        "Some items in this order couldn't be delivered. Everything that was delivered is yours; the Credits for the undelivered items were automatically refunded to your balance.";
      refundNote =
        "If something looks off, contact support with the checkout number below.";
      actions = (
        <>
          <Link
            to={href("/marketplace/account")}
            className="btn btn--primary btn--md"
            style={LINK_BTN}
          >
            View your items
          </Link>
          <Link
            to={href("/shop")}
            className="btn btn--secondary btn--md"
            style={LINK_BTN}
          >
            Back to marketplace
          </Link>
        </>
      );
    }
  } else if (insufficient) {
    title = "Not enough Credits";
    detail = "You don't have enough Credits to complete this purchase.";
    refundNote = "You were not charged.";
    actions = (
      <>
        <Button variant="primary" onClick={onRetry}>
          Review &amp; add Credits
        </Button>
        <Link
          to={href("/marketplace/packs")}
          className="btn btn--secondary btn--md"
          style={LINK_BTN}
        >
          Buy a Credits pack
        </Link>
      </>
    );
  } else if (!!error && /no open marketplace listing/i.test(error)) {
    title = "An item just sold out";
    detail =
      "Someone bought the last available copy of an item in this order before your checkout completed. Nothing was charged.";
    refundNote = null;
    actions = (
      <>
        <Link
          to={cancel.href}
          className="btn btn--primary btn--md"
          style={LINK_BTN}
        >
          {cancel.label}
        </Link>
        <Link
          to={href("/shop")}
          className="btn btn--secondary btn--md"
          style={LINK_BTN}
        >
          Back to marketplace
        </Link>
      </>
    );
  } else {
    title = "Checkout failed";
    detail = error ?? "Something went wrong while processing your purchase.";
    actions = (
      <>
        <Button variant="primary" onClick={onRetry}>
          Try again
        </Button>
        <Link
          to={cancel.href}
          className="btn btn--secondary btn--md"
          style={LINK_BTN}
        >
          {cancel.label}
        </Link>
      </>
    );
  }

  return (
    <MkCheckoutCard tone="error">
      <p role="alert" className="mkco__lead mkco__lead--error">
        {title}
      </p>
      <p className="mkco__text">{detail}</p>
      {refundNote && <p className="mkco__muted">{refundNote}</p>}
      {checkoutId != null && <p className="mkco__muted">Checkout #{checkoutId}</p>}
      <MkCheckoutActions>{actions}</MkCheckoutActions>
    </MkCheckoutCard>
  );
}

export const CartGlyph = (
  <svg viewBox="0 0 24 24" width="28" height="28" fill="none" aria-hidden="true">
    <path
      d="M3 4h2l2.4 11.2a1.5 1.5 0 0 0 1.47 1.2h8.26a1.5 1.5 0 0 0 1.47-1.18L21 8H6"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
    <circle cx="9.5" cy="20" r="1.2" fill="currentColor" />
    <circle cx="17.5" cy="20" r="1.2" fill="currentColor" />
  </svg>
);
