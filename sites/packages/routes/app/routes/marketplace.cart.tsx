import type { ReactNode } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import Button from "@ui/atoms/Button";
import {
  MkCheckoutFrame,
  MkCheckoutCard,
  MkCheckoutActions,
  MkCheckoutSummary,
} from "@ui/marketplace/pages/MkCheckout";

import { openSignIn } from "@features/components/auth/signin-store";
import { useAuth } from "@data/lib/auth/index";
import {
  cartCount,
  fetchCart,
  removeCartItem,
  type Cart,
  type CartLine,
} from "@data/lib/catalyst/marketplace/cart";
import {
  fallbackDisplayLine,
  useDisplayLines,
} from "@data/lib/catalyst/marketplace/cart-display";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.cart";

const STORY = "marketplace-cart";
const TITLE = "Your cart";

const EMPTY_FOCUS = "__empty__";
const lineKey = (l: Pick<CartLine, "collection" | "itemId">) =>
  `${l.collection ?? ""}-${l.itemId}`;

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const payload = { sid };
  return wrap(payload);
}

export default function MarketplaceCartRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData as { sid: string };
  const auth = useAuth();
  const [cart, setCart] = useState<Cart | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const [focusKey, setFocusKey] = useState<string | null>(null);
  const removeBtnRefs = useRef(new Map<string, HTMLButtonElement>());
  const emptyLeadRef = useRef<HTMLParagraphElement | null>(null);

  useEffect(() => {
    if (!auth.isConnected || !auth.identity) return;
    let cancelled = false;
    fetchCart(auth.identity)
      .then((c) => !cancelled && setCart(c))
      .catch(() => !cancelled && setCart(null));
    return () => {
      cancelled = true;
    };
  }, [auth.isConnected, auth.identity]);

  useCartViewed(d.sid, cart ?? null);
  const items = useMemo(() => (cart ? cart.items : []), [cart]);
  const displayLines = useDisplayLines(items);
  const displayByKey = useMemo(
    () => new Map(displayLines.map((l) => [l.key, l])),
    [displayLines],
  );

  useEffect(() => {
    if (!focusKey) return;
    const el =
      focusKey === EMPTY_FOCUS
        ? emptyLeadRef.current
        : removeBtnRefs.current.get(focusKey);
    if (el) {
      el.focus();
      setFocusKey(null);
    }
  }, [focusKey, cart]);

  async function onRemove(item: CartLine, name: string) {
    if (!auth.isConnected || !auth.identity || !item.collection) return;
    setBusy(true);
    setError(null);
    try {
      const removedIndex = cart
        ? cart.items.findIndex((l) => lineKey(l) === lineKey(item))
        : -1;
      const next = await removeCartItem(
        auth.identity,
        item.collection,
        item.itemId,
      );
      setCart(next);
      setAnnouncement(`${name} removed from cart`);
      setFocusKey(
        next.items.length === 0
          ? EMPTY_FOCUS
          : lineKey(
              next.items[
                Math.min(Math.max(removedIndex, 0), next.items.length - 1)
              ],
            ),
      );
      track(
        "cart_remove",
        { item_id: item.itemId, collection: item.collection },
        { sid: d.sid, story: STORY },
      );
    } catch (err) {
      setError((err as Error)?.message ?? "Could not remove the item.");
    } finally {
      setBusy(false);
    }
  }

  let body: ReactNode;
  if (!auth.isConnected) {
    body = (
      <MkCheckoutCard>
        <p className="mkco__lead">Sign in to see your cart</p>
        <p className="mkco__muted">
          Your cart lives with your account, so it follows you across devices.
        </p>
        <MkCheckoutActions>
          <Button variant="primary" onClick={() => openSignIn()}>
            Sign in
          </Button>
          <Link to={href("/shop")} className="mkco__link">
            Continue shopping
          </Link>
        </MkCheckoutActions>
      </MkCheckoutCard>
    );
  } else if (cart === undefined) {
    body = (
      <MkCheckoutCard busy>
        <p className="mkco__muted">Loading your cart{"\u{2026}"}</p>
      </MkCheckoutCard>
    );
  } else if (cart === null) {
    body = (
      <MkCheckoutCard tone="error">
        <p className="mkco__lead">We couldn&apos;t load your cart</p>
        <p className="mkco__muted">Please try again in a moment.</p>
        <MkCheckoutActions>
          <Link to={href("/shop")} className="btn btn--primary btn--md">
            Browse the shop
          </Link>
        </MkCheckoutActions>
      </MkCheckoutCard>
    );
  } else if (cart.items.length === 0) {
    body = (
      <MkCheckoutCard>
        <p className="mkco__lead" tabIndex={-1} ref={emptyLeadRef}>
          Your cart is empty
        </p>
        <p className="mkco__muted">
          Add items from the shop, then check out here.
        </p>
        <MkCheckoutActions>
          <Link to={href("/shop")} className="btn btn--primary btn--md">
            Browse the shop
          </Link>
        </MkCheckoutActions>
      </MkCheckoutCard>
    );
  } else {
    body = (
      <MkCheckoutCard>
        {error && (
          <p role="alert" className="mkco__alert">
            {error}
          </p>
        )}
        <MkCheckoutSummary
          heading={`${cartCount(cart)} item${cartCount(cart) === 1 ? "" : "s"}`}
          lines={cart.items.map((item) => {
            const key = lineKey(item);
            const line = displayByKey.get(key) ?? fallbackDisplayLine(item);
            return {
              ...line,
              action: (
                <button
                  type="button"
                  ref={(el) => {
                    if (el) removeBtnRefs.current.set(key, el);
                    else removeBtnRefs.current.delete(key);
                  }}
                  disabled={busy}
                  onClick={() => onRemove(item, line.name)}
                  aria-label={`Remove ${line.name} from cart`}
                >
                  Remove
                </button>
              ),
            };
          })}
          total={cart.totalCredits}
        />
        <MkCheckoutActions between>
          <Link to={href("/shop")} className="mkco__link">
            Continue shopping
          </Link>
          <Link
            to={href("/marketplace/checkout")}
            prefetch="intent"
            className="btn btn--primary btn--md"
          >
            Proceed to checkout
          </Link>
        </MkCheckoutActions>
      </MkCheckoutCard>
    );
  }

  return (
    <MkCheckoutFrame title={TITLE} back={{ href: "/shop", label: "Back to the shop" }}>
      <p className="mkco__srlive" aria-live="polite">
        {announcement}
      </p>
      {body}
    </MkCheckoutFrame>
  );
}

function useCartViewed(sid: string, cart: Cart | null) {
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    if (!cart) return;
    const key = `${cart.items.length}:${cart.totalCredits}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    track(
      "cart_viewed",
      { item_count: cart.items.length, total_credits: cart.totalCredits },
      { sid, story: STORY },
    );
  }, [sid, cart]);
}
