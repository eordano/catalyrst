import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { href } from "@core/lib/router/routes";

import Button from "@ui/atoms/Button";
import EmptyState from "@ui/components/EmptyState";
import { MkCheckoutFrame } from "@ui/marketplace/pages/MkCheckout";

import { openSignIn } from "@features/components/auth/signin-store";
import { CartGlyph, LoadingScreen } from "@features/components/marketplace/CheckoutCards";
import DevStatePreview from "@features/components/marketplace/CheckoutDevPreview";
import CheckoutWizard from "@features/components/marketplace/CheckoutWizard";
import ResumeCheckout from "@features/components/marketplace/ResumeCheckout";
import { useAuth } from "@data/lib/auth/index";
import type { AuthIdentity } from "@data/lib/auth/index";
import {
  addCartItem,
  fetchCart,
  parseItemRef,
  removeCartItem,
  type Cart,
  type CartLine,
} from "@data/lib/catalyst/marketplace/cart";
import { useDisplayLines } from "@data/lib/catalyst/marketplace/cart-display";
import {
  applyFreshQuotes,
  fetchBalance,
  quoteExpressItem,
  type Balance,
} from "@data/lib/catalyst/marketplace/checkout";
import {
  makeExpressRun,
  makeRun,
  type CancelTarget,
  type CreateRun,
  type RefreshLines,
} from "@data/lib/catalyst/marketplace/checkout-run";
import { loadCheckout } from "@data/lib/catalyst/marketplace/checkout.server";
import { tryQuoteCreditItems } from "@data/lib/catalyst/marketplace/credit-quotes";
import {
  clearPendingCheckout,
  getPendingCheckout,
  type PendingCheckout,
} from "@data/lib/catalyst/marketplace/pending-checkout";
import {
  clearPendingTopup,
  getPendingTopup,
} from "@data/lib/catalyst/marketplace/pending-topup";
import {
  intentLineFromCart,
  type IntentLine,
} from "@data/lib/catalyst/marketplace/purchase-intent";
import { redeemManaTopup } from "@data/lib/catalyst/marketplace/topup";
import { sidLoader } from "@core/lib/experiments/story-loader";
import type { TrackContext } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.checkout";

const STORY = "marketplace/checkout";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const load = await loadCheckout().catch(() => null);

  const payload = {
    sid,
    balance: load ? load.balance : null,
    isFixture: load ? load.isFixture : true,
  };

  return wrap(payload);
}

const CART_CANCEL: CancelTarget = { label: "Back to cart", href: "/marketplace/cart" };
const EXPRESS_CANCEL: CancelTarget = { label: "Cancel", href: "/shop" };

export default function MarketplaceCheckoutRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const auth = useAuth();
  const [searchParams] = useSearchParams();
  const expressParam = searchParams.get("express");
  const stateParam = searchParams.get("state");
  const tourParam = searchParams.get("tour");

  if (import.meta.env.DEV && stateParam !== null) {
    return <DevStatePreview state={stateParam} tour={tourParam !== null} />;
  }
  const payParam = searchParams.get("pay");
  const payPreselect =
    payParam === "card" || payParam === "mana" ? payParam : undefined;

  const [balance, setBalance] = useState<Balance | null>(null);

  useEffect(() => {
    if (!auth.isConnected) openSignIn();
  }, [auth.isConnected]);

  const identity = auth.identity;
  const refreshBalance = useCallback(async () => {
    if (!identity) return;
    try {
      setBalance(await fetchBalance(identity, identity.signer));
    } catch {
    }
  }, [identity]);

  useEffect(() => {
    if (!auth.isConnected || !auth.identity) return;
    let cancelled = false;
    const id = auth.identity;
    fetchBalance(id, id.signer)
      .then((b) => !cancelled && setBalance(b))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [auth.isConnected, auth.identity]);

  useEffect(() => {
    if (!identity) return;
    const pending = getPendingTopup(identity.signer);
    if (!pending) return;
    let cancelled = false;
    (async () => {
      for (let i = 0; i < 40 && !cancelled; i++) {
        try {
          const res = await redeemManaTopup(identity, pending.txHash);
          if (res.state === "granted") {
            clearPendingTopup(identity.signer);
            await refreshBalance();
            return;
          }
        } catch (err) {
          const status = (err as { status?: number })?.status;
          if (status === 422 || status === 403) {
            clearPendingTopup(identity.signer);
          }
          return;
        }
        await new Promise((r) => setTimeout(r, 4000));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [identity, refreshBalance]);

  const signer = auth.identity?.signer;
  const [resume, setResume] = useState<PendingCheckout | null | undefined>(
    undefined,
  );
  const [resumeDismissed, setResumeDismissed] = useState(false);
  useEffect(() => {
    setResume(getPendingCheckout(signer));
  }, [signer]);

  if (!auth.isConnected || !auth.identity) {
    return (
      <MkCheckoutFrame>
        <EmptyState
          variant="screen"
          icon={CartGlyph}
          title="Sign in to check out"
          subtitle="Sign in to spend Credits and complete your purchase."
          actions={
            <Button variant="primary" onClick={() => openSignIn()}>
              Sign in
            </Button>
          }
        />
      </MkCheckoutFrame>
    );
  }

  const trackCtx: TrackContext = { sid: d.sid, story: STORY };

  if (resume === undefined) {
    return <LoadingScreen label={"Checking for a purchase in progress\u{2026}"} />;
  }

  if (resume && !resumeDismissed) {
    return (
      <MkCheckoutFrame>
        <ResumeCheckout
          identity={auth.identity}
          checkoutId={resume.checkoutId}
          onDismiss={() => {
            clearPendingCheckout(auth.identity!.signer);
            setResumeDismissed(true);
          }}
        />
      </MkCheckoutFrame>
    );
  }

  if (expressParam !== null) {
    return (
      <ExpressCheckout
        identity={auth.identity}
        raw={expressParam}
        balance={balance}
        refreshBalance={refreshBalance}
        payPreselect={payPreselect}
        trackCtx={trackCtx}
      />
    );
  }

  return (
    <CartCheckout
      identity={auth.identity}
      balance={balance}
      refreshBalance={refreshBalance}
      payPreselect={payPreselect}
      trackCtx={trackCtx}
    />
  );
}

function CartCheckout({
  identity,
  balance,
  refreshBalance,
  payPreselect,
  trackCtx,
}: {
  identity: AuthIdentity;
  balance: Balance | null;
  refreshBalance: () => Promise<void>;
  payPreselect?: "card" | "mana";
  trackCtx: TrackContext;
}) {
  const [cart, setCart] = useState<Cart | null>(null);
  const [cartError, setCartError] = useState<string | null>(null);
  const createRun = useMemo<CreateRun>(
    () => (getSigned) => makeRun(identity, getSigned),
    [identity],
  );
  const source = useMemo<CartLine[]>(() => cart?.items ?? [], [cart]);
  const intentLines = useMemo(() => source.map(intentLineFromCart), [source]);
  const displayLines = useDisplayLines(source);

  const refreshLines = useCallback<RefreshLines>(async () => {
    const current = await fetchCart(identity).catch(() => null);
    if (!current || current.items.length === 0) return null;
    const refs = current.items.map((l) => {
      const collection = intentLineFromCart(l).collection;
      return collection ? { itemId: l.itemId, collection } : null;
    });
    if (refs.some((r) => r === null)) return null;
    const credits = await tryQuoteCreditItems(refs);
    const applied = applyFreshQuotes(current.items, credits);
    if (!applied) return null;
    setCart({ ...current, items: applied.lines, totalCredits: applied.totalCredits });
    return {
      totalCredits: applied.totalCredits,
      intentLines: applied.lines.map(intentLineFromCart),
    };
  }, [identity]);

  useEffect(() => {
    let cancelled = false;
    setCartError(null);
    fetchCart(identity)
      .then((c) => !cancelled && setCart(c))
      .catch((err) => {
        if (!cancelled) {
          setCartError(
            (err as Error)?.message ??
              "We couldn't load your cart. Please try again.",
          );
        }
      });
    return () => {
      cancelled = true;
    };
  }, [identity]);

  if (cartError) {
    return (
      <MkCheckoutFrame>
        <EmptyState
          variant="screen"
          tone="error"
          icon={CartGlyph}
          title="We couldn't load your cart"
          subtitle={cartError}
          actions={
            <Link to={href("/marketplace/cart")} className="btn btn--secondary btn--md">
              Back to your cart
            </Link>
          }
        />
      </MkCheckoutFrame>
    );
  }

  if (cart === null) {
    return <LoadingScreen label={"Loading your cart\u{2026}"} />;
  }

  if (cart.items.length === 0) {
    return (
      <MkCheckoutFrame>
        <EmptyState
          variant="screen"
          icon={CartGlyph}
          title="Your cart is empty"
          subtitle="Add items from the shop, then come back to check out."
          actions={
            <Link to={href("/shop")} className="btn btn--primary btn--md">
              Browse the shop
            </Link>
          }
        />
      </MkCheckoutFrame>
    );
  }

  return (
    <CheckoutWizard
      createRun={createRun}
      identity={identity}
      buyer={identity.signer}
      intentLines={intentLines}
      totalCredits={cart.totalCredits}
      itemCount={cart.items.length}
      balance={balance}
      refreshBalance={refreshBalance}
      payPreselect={payPreselect}
      trackCtx={trackCtx}
      cancel={CART_CANCEL}
      lines={displayLines}
      refreshLines={refreshLines}
    />
  );
}

function ExpressCheckout({
  identity,
  raw,
  balance,
  refreshBalance,
  payPreselect,
  trackCtx,
}: {
  identity: AuthIdentity;
  raw: string;
  balance: Balance | null;
  refreshBalance: () => Promise<void>;
  payPreselect?: "card" | "mana";
  trackCtx: TrackContext;
}) {
  const ref = useMemo(() => parseItemRef(raw), [raw]);
  const [line, setLine] = useState<CartLine | null>(null);
  const [error, setError] = useState<string | null>(null);
  const createRun = useMemo<CreateRun | null>(
    () => (ref ? (getSigned) => makeExpressRun(identity, ref, getSigned) : null),
    [identity, ref],
  );
  const intentLines = useMemo<IntentLine[]>(
    () => (ref ? [{ collection: ref.collection, itemId: ref.itemId, qty: 1 }] : []),
    [ref],
  );
  const source = useMemo<CartLine[]>(() => (line ? [line] : []), [line]);
  const displayLines = useDisplayLines(source);

  const addedForQuote = useRef(false);
  const committed = useRef(false);

  const refreshLines = useCallback<RefreshLines>(async () => {
    if (!ref) return null;
    const next = await addCartItem(identity, ref, 1);
    const fresh = next.items.find(
      (l) =>
        l.itemId === ref.itemId &&
        (l.collection ?? "").toLowerCase() === ref.collection,
    );
    if (!fresh) return null;
    setLine(fresh);
    return {
      totalCredits: fresh.unitPriceCredits,
      intentLines: [{ collection: ref.collection, itemId: ref.itemId, qty: 1 }],
    };
  }, [identity, ref]);

  useEffect(() => {
    if (!ref) {
      setError("This item can't be bought directly.");
      return;
    }
    let cancelled = false;
    setError(null);
    setLine(null);
    quoteExpressItem(identity, ref)
      .then((q) => {
        if (q) addedForQuote.current = q.added;
        if (cancelled) return;
        if (q) setLine(q.line);
        else setError("This item isn't available to buy right now.");
      })
      .catch((err) => {
        if (!cancelled) {
          setError(
            (err as Error)?.message ?? "We couldn't load this item. Please try again.",
          );
        }
      });
    return () => {
      cancelled = true;
    };
  }, [identity, ref]);

  useEffect(() => {
    return () => {
      if (ref && addedForQuote.current && !committed.current) {
        removeCartItem(identity, ref.collection, ref.itemId).catch(() => {
        });
      }
    };
  }, [identity, ref]);

  if (error) {
    return (
      <MkCheckoutFrame>
        <EmptyState
          variant="screen"
          tone="error"
          icon={CartGlyph}
          title="This item isn't available"
          subtitle={error}
          actions={
            <>
              {ref ? (
                <Link
                  to={`/marketplace/${encodeURIComponent(raw)}`}
                  className="btn btn--primary btn--md"
                >
                  View the item
                </Link>
              ) : null}
              <Link to={href("/shop")} className="btn btn--secondary btn--md">
                Back to marketplace
              </Link>
            </>
          }
        />
      </MkCheckoutFrame>
    );
  }

  if (line === null || createRun === null) {
    return <LoadingScreen label={"Loading this item\u{2026}"} />;
  }

  return (
    <CheckoutWizard
      createRun={createRun}
      identity={identity}
      buyer={identity.signer}
      intentLines={intentLines}
      totalCredits={line.unitPriceCredits}
      itemCount={1}
      balance={balance}
      refreshBalance={refreshBalance}
      payPreselect={payPreselect}
      trackCtx={trackCtx}
      cancel={EXPRESS_CANCEL}
      lines={displayLines}
      refreshLines={refreshLines}
      onSpendCommitted={() => {
        committed.current = true;
      }}
    />
  );
}
