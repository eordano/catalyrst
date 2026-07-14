import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";
import { loadStripe } from "@stripe/stripe-js/pure";
import type { Stripe } from "@stripe/stripe-js";
import {
  Elements,
  PaymentElement,
  useElements,
  useStripe,
} from "@stripe/react-stripe-js";

import Button from "@ui/atoms/Button";
import {
  MkCheckoutFrame,
  MkCheckoutCard,
  MkCheckoutActions,
} from "@ui/marketplace/pages/MkCheckout";

import { useAuth } from "@data/lib/auth/index";
import {
  createPackIntent,
  mockPurchasePack,
  formatCredits,
  formatPrice,
  type Pack,
} from "@data/lib/catalyst/marketplace/packs";
import { loadPacks } from "@data/lib/catalyst/marketplace/packs.server";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { openSignIn } from "@features/components/auth/signin-store";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.packs";

const STORY = "marketplace-packs";
const TITLE = "Buy Marketplace Credits";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const load = await loadPacks({ signal: request.signal }).catch(() => null);

  const payload = {
    sid,
    packs: load?.data ?? [],
    packsSource: load?.source ?? "unavailable",
    packsReason: load?.reason ?? null,
    stripeKey:
      (typeof process !== "undefined"
        ? process.env?.STRIPE_PUBLISHABLE_KEY
        : undefined) ?? null,
  };

  return wrap(payload);
}

const stripeCache = new Map<string, Promise<Stripe | null>>();
function getStripe(key: string): Promise<Stripe | null> {
  let p = stripeCache.get(key);
  if (!p) {
    p = loadStripe(key);
    stripeCache.set(key, p);
  }
  return p;
}

export default function MarketplacePacksRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const auth = useAuth();
  const [active, setActive] = useState<{ sku: string; clientSecret: string } | null>(
    null,
  );
  const [mockPack, setMockPack] = useState<Pack | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  usePacksViewed(d.sid, d.packs, d.packsSource);

  const stripePromise = useMemo(
    () => (d.stripeKey ? getStripe(d.stripeKey) : null),
    [d.stripeKey],
  );

  async function onBuy(pack: Pack) {
    setError(null);
    if (!auth.isConnected || !auth.identity) {
      openSignIn();
      return;
    }
    if (!d.stripeKey) {
      setMockPack(pack);
      return;
    }
    setBusy(pack.sku);
    try {
      const intent = await createPackIntent(auth.identity, pack.sku);
      track(
        "pack_purchase_started",
        { sku: pack.sku, credits: pack.credits, price_cents: pack.priceCents },
        { sid: d.sid, story: STORY },
      );
      setActive({ sku: pack.sku, clientSecret: intent.clientSecret });
    } catch (err) {
      setError((err as Error)?.message ?? "Could not start the purchase.");
    } finally {
      setBusy(null);
    }
  }

  if (mockPack) {
    return (
      <MkCheckoutFrame title={TITLE}>
        <MockCardForm
          sid={d.sid}
          pack={mockPack}
          onCancel={() => setMockPack(null)}
        />
      </MkCheckoutFrame>
    );
  }

  if (active && stripePromise) {
    const pack = d.packs.find((p) => p.sku === active.sku) ?? null;
    return (
      <MkCheckoutFrame title="Complete your purchase">
        <Elements
          stripe={stripePromise}
          options={{ clientSecret: active.clientSecret }}
        >
          <PackPaymentForm
            sid={d.sid}
            pack={pack}
            onCancel={() => setActive(null)}
          />
        </Elements>
      </MkCheckoutFrame>
    );
  }

  return (
    <MkCheckoutFrame title={TITLE}>
      <p className="mkco__intro">
        1 Credit = $0.10. Spend Credits on Wearables &amp; Emotes.
      </p>

      {error && (
        <p role="alert" className="mkco__alert">
          {error}
        </p>
      )}

      <div className="mkco__packs">
        {d.packs.map((pack) => (
          <article key={pack.sku} className="mkco__pack">
            <h2 className="mkco__packtitle">{pack.title}</h2>
            <p className="mkco__packcredits">{formatCredits(pack.credits)} Credits</p>
            <p className="mkco__packprice">
              {formatPrice(pack.priceCents, pack.currency)}
            </p>
            <div className="mkco__packbuy">
              <Button
                variant="primary"
                disabled={busy === pack.sku}
                onClick={() => onBuy(pack)}
              >
                {busy === pack.sku ? "Starting…" : "Buy"}
              </Button>
            </div>
          </article>
        ))}
        {d.packs.length === 0 &&
          (d.packsSource === "unavailable" ? (
            <p role="alert" className="mkco__alert">
              We couldn&apos;t load the Credit packs just now. That&apos;s a
              problem on our side — it doesn&apos;t mean packs are sold out.
              Please reload in a moment.
            </p>
          ) : (
            <p className="mkco__muted">No Credit packs are available right now.</p>
          ))}
      </div>
    </MkCheckoutFrame>
  );
}

function MockCardForm({
  sid,
  pack,
  onCancel,
}: {
  sid: string;
  pack: Pack;
  onCancel: () => void;
}) {
  const auth = useAuth();
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [granted, setGranted] = useState<string | null>(null);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!auth.identity) {
      setMessage("Sign in to buy Credits.");
      return;
    }
    setSubmitting(true);
    setMessage(null);
    try {
      const res = await mockPurchasePack(auth.identity, pack.sku);
      track(
        "pack_purchased_mock",
        { sku: pack.sku, credits: res.creditsGranted },
        { sid, story: STORY },
      );
      setGranted(res.creditsGranted);
    } catch (err) {
      setMessage((err as Error)?.message ?? "Mock purchase failed.");
    } finally {
      setSubmitting(false);
    }
  }

  if (granted) {
    return (
      <MkCheckoutCard tone="success">
        <div className="mkco__badge">MOCK · no real charge</div>
        <p className="mkco__lead mkco__lead--success">
          {formatCredits(granted)} Credits added to your balance.
        </p>
        <p className="mkco__muted">
          This was a simulated card payment — no money was charged. The Credits
          are real and ready to spend at checkout.
        </p>
        <MkCheckoutActions>
          <Link to={href("/shop")} className="btn btn--primary btn--md">
            Continue shopping
          </Link>
          <Link to={href("/marketplace/credits")} className="mkco__link">
            View your Credits
          </Link>
        </MkCheckoutActions>
      </MkCheckoutCard>
    );
  }

  return (
    <form onSubmit={onSubmit}>
      <MkCheckoutCard>
        <div className="mkco__badge">MOCK payment · test card · no real charge</div>
        <p className="mkco__lead">Pay with card</p>
        <p className="mkco__muted">
          {pack.title} — {formatCredits(pack.credits)} Credits ·{" "}
          {formatPrice(pack.priceCents, pack.currency)}
        </p>

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
          <label className="mkco__field" style={{ width: 120 }}>
            <span className="mkco__fieldlabel">CVC</span>
            <input
              className="mkco__fieldinput"
              placeholder="123"
              defaultValue="123"
              aria-label="CVC (test)"
            />
          </label>
        </div>

        {message && (
          <p role="alert" className="mkco__alert">
            {message}
          </p>
        )}

        <MkCheckoutActions>
          <Button variant="primary" type="submit" disabled={submitting}>
            {submitting
              ? "Processing…"
              : `Pay ${formatPrice(pack.priceCents, pack.currency)} (mock)`}
          </Button>
          <Button variant="secondary" type="button" onClick={onCancel}>
            Cancel
          </Button>
        </MkCheckoutActions>
        <p className="mkco__note">
          Card payments aren&apos;t live yet. This is a test flow — no card is
          charged and the details above are ignored.
        </p>
      </MkCheckoutCard>
    </form>
  );
}

function PackPaymentForm({
  sid,
  pack,
  onCancel,
}: {
  sid: string;
  pack: Pack | null;
  onCancel: () => void;
}) {
  const stripe = useStripe();
  const elements = useElements();
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!stripe || !elements) return;
    setSubmitting(true);
    setMessage(null);
    const { error, paymentIntent } = await stripe.confirmPayment({
      elements,
      redirect: "if_required",
    });
    setSubmitting(false);
    if (error) {
      setMessage(error.message ?? "Payment failed.");
      return;
    }
    if (paymentIntent && paymentIntent.status === "succeeded") {
      setDone(true);
      track(
        "pack_purchased",
        {
          sku: pack?.sku ?? null,
          credits: pack?.credits ?? null,
          payment_intent: paymentIntent.id,
        },
        { sid, story: STORY },
      );
    } else {
      setMessage(`Payment status: ${paymentIntent?.status ?? "processing"}.`);
    }
  }

  if (done) {
    return (
      <MkCheckoutCard tone="success">
        <p className="mkco__lead mkco__lead--success">Payment received</p>
        <p className="mkco__muted">
          Your Credits will appear once Stripe confirms the charge.
        </p>
        <MkCheckoutActions>
          <Link to={href("/marketplace/credits")} className="btn btn--primary btn--md">
            View your Credits
          </Link>
        </MkCheckoutActions>
      </MkCheckoutCard>
    );
  }

  return (
    <form onSubmit={onSubmit}>
      <MkCheckoutCard>
        <PaymentElement />
        {message && (
          <p role="alert" className="mkco__alert">
            {message}
          </p>
        )}
        <MkCheckoutActions>
          <Button variant="primary" type="submit" disabled={!stripe || submitting}>
            {submitting ? "Processing…" : "Pay"}
          </Button>
          <Button variant="secondary" type="button" onClick={onCancel}>
            Cancel
          </Button>
        </MkCheckoutActions>
      </MkCheckoutCard>
    </form>
  );
}

function usePacksViewed(
  sid: string,
  packs: Pack[],
  source: "live" | "empty" | "unavailable",
) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      "pack_viewed",
      { count: packs.length, skus: packs.map((p) => p.sku), source },
      { sid, story: STORY },
    );
  }, [sid, packs, source]);
}
