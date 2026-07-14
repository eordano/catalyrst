import MarketplaceCheckoutRoute from "../routes/marketplace.checkout";
import cartFx from "@data/fixtures/route-cart.json";
import { expect, waitFor, within } from "@ui/docs/sb";
import { CATALYST_BASE, HttpResponse, catalystGet, http, routeStory } from "./lib";
import { SignedInScope, STORY_SIGNER } from "./signed-in";

const cart = cartFx.cart;
const catalogRows: Record<string, unknown> = cartFx.catalogRows;

const loaderData = { sid: "story-sid", balance: null, isFixture: false };

const catalogLookup = http.get(`${CATALYST_BASE}/market/v1/catalog`, ({ request }) => {
  const p = new URL(request.url).searchParams;
  const row = catalogRows[`${p.get("contractAddress") ?? ""}-${p.get("itemId") ?? ""}`];
  return HttpResponse.json({ data: row ? [row] : [], total: row ? 1 : 0 });
});

const cartOk = catalystGet("/credits/cart", cart);

const balanceOf = (available: string) =>
  catalystGet(`/credits/wallet/${STORY_SIGNER}/balance`, {
    address: STORY_SIGNER,
    available,
  });

const paymentsConfig = catalystGet("/v1/payments/config", {
  chainId: 137,
  enabled: true,
  manaToken: "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
  payTo: "0xe50a4c473bca807a1e66381abc4280dcdf718316",
});

const manaQuote = catalystGet("/credits/topup/mana/quote", {
  credits: "2",
  weiSuggested: "2911457441342695692",
  manaUsd: "0.070068",
});

const CheckoutRoute = routeStory({
  Component: MarketplaceCheckoutRoute,
  path: "/marketplace/checkout",
  loaderData,
});

export default {
  title: "Routes/MarketplaceCheckout",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const SignedOut = { render: CheckoutRoute };

export const ReviewTwoItems = {
  render: () => (
    <SignedInScope>
      <CheckoutRoute />
    </SignedInScope>
  ),
  parameters: {
    msw: { handlers: [cartOk, catalogLookup, balanceOf("10")] },
  },
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    const canvas = within(canvasElement);
    await waitFor(
      () => expect(canvas.getByText(/Spend 2 Credits on 2 items/)).toBeVisible(),
      { timeout: 10000 },
    );
    await waitFor(
      () => expect(canvas.getByText(/Your balance: 10 Credits/)).toBeVisible(),
      { timeout: 10000 },
    );
  },
};

export const InsufficientBalance = {
  render: () => (
    <SignedInScope>
      <CheckoutRoute />
    </SignedInScope>
  ),
  parameters: {
    msw: {
      handlers: [cartOk, catalogLookup, balanceOf("0"), paymentsConfig, manaQuote],
    },
  },
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    const canvas = within(canvasElement);
    await waitFor(
      () => expect(canvas.getByText(/Spend 2 Credits on 2 items/)).toBeVisible(),
      { timeout: 10000 },
    );
  },
};

export const EmptyCart = {
  render: () => (
    <SignedInScope>
      <CheckoutRoute />
    </SignedInScope>
  ),
  parameters: {
    msw: {
      handlers: [
        catalystGet("/credits/cart", { ...cart, items: [], totalCredits: "0" }),
        balanceOf("10"),
      ],
    },
  },
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    const canvas = within(canvasElement);
    await waitFor(() => expect(canvas.getByText("Your cart is empty")).toBeVisible(), {
      timeout: 10000,
    });
  },
};

export const CartLoadFailed = {
  render: () => (
    <SignedInScope>
      <CheckoutRoute />
    </SignedInScope>
  ),
  parameters: {
    msw: {
      handlers: [
        catalystGet("/credits/cart", { message: "cart backend unavailable" }, { status: 500 }),
        balanceOf("10"),
      ],
    },
  },
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    const canvas = within(canvasElement);
    await waitFor(
      () => expect(canvas.getByText("We couldn't load your cart")).toBeVisible(),
      { timeout: 10000 },
    );
  },
};
