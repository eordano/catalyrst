import MarketplaceCartRoute from "../routes/marketplace.cart";
import cartFx from "@data/fixtures/route-cart.json";
import { expect, waitFor, within } from "@ui/docs/sb";
import { CATALYST_BASE, HttpResponse, catalystGet, http, routeStory } from "./lib";
import { SignedInScope } from "./signed-in";

const cart = cartFx.cart;
const catalogRows: Record<string, unknown> = cartFx.catalogRows;

const catalogLookup = http.get(`${CATALYST_BASE}/market/v1/catalog`, ({ request }) => {
  const p = new URL(request.url).searchParams;
  const row = catalogRows[`${p.get("contractAddress") ?? ""}-${p.get("itemId") ?? ""}`];
  return HttpResponse.json({ data: row ? [row] : [], total: row ? 1 : 0 });
});

const removeItem = http.delete(
  `${CATALYST_BASE}/credits/cart/items/:collection/:itemId`,
  ({ params }) => {
    const items = cart.items.filter(
      (l) =>
        !(l.collection === String(params.collection) && l.itemId === String(params.itemId)),
    );
    const total = items.reduce((n, l) => n + Number(l.unitPriceCredits) * l.qty, 0);
    return HttpResponse.json({ ...cart, items, totalCredits: String(total) });
  },
);

const CartRoute = routeStory({
  Component: MarketplaceCartRoute,
  path: "/marketplace/cart",
  loaderData: { sid: "story-sid" },
});

export default {
  title: "Routes/MarketplaceCart",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const SignedOut = { render: CartRoute };

export const WithItems = {
  render: () => (
    <SignedInScope>
      <CartRoute />
    </SignedInScope>
  ),
  parameters: {
    msw: { handlers: [catalystGet("/credits/cart", cart), catalogLookup, removeItem] },
  },
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    const canvas = within(canvasElement);
    await waitFor(() => expect(canvas.getByText("ENO T-Shirt")).toBeVisible(), {
      timeout: 10000,
    });
  },
};

export const Empty = {
  render: () => (
    <SignedInScope>
      <CartRoute />
    </SignedInScope>
  ),
  parameters: {
    msw: {
      handlers: [catalystGet("/credits/cart", { ...cart, items: [], totalCredits: "0" })],
    },
  },
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    const canvas = within(canvasElement);
    await waitFor(() => expect(canvas.getByText("Your cart is empty")).toBeVisible(), {
      timeout: 10000,
    });
  },
};

export const LoadFailed = {
  render: () => (
    <SignedInScope>
      <CartRoute />
    </SignedInScope>
  ),
  parameters: {
    msw: {
      handlers: [
        catalystGet("/credits/cart", { message: "cart backend unavailable" }, { status: 500 }),
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
