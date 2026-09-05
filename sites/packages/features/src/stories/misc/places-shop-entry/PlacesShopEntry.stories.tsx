// Storybook-only fixtures: the live fetchCatalog loader does not run in
// Storybook, so these feed representative display data. Production stays on
// the real loaders + schema-honesty (see the places route loader).
import type { Meta, StoryObj } from "@ui/docs/sb";
import { expect, fn, userEvent, waitFor } from "@ui/docs/sb";

import PlaceCard from "@ui/components/PlaceCard";
import "@ui/explorer/pages/places.css";

import type { CollectibleCard } from "@data/lib/catalyst/marketplace/index";
import PlacesShopEntry from "./PlacesShopEntry";

function swatch(from: string, to: string): string {
  const svg =
    `<svg xmlns='http://www.w3.org/2000/svg' width='56' height='56'>` +
    `<defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>` +
    `<stop offset='0' stop-color='${from}'/><stop offset='1' stop-color='${to}'/>` +
    `</linearGradient></defs><rect width='56' height='56' rx='12' fill='url(#g)'/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

const railFixtures: CollectibleCard[] = [
  {
    id: "0x1a2b:0",
    name: "Aurora Runner Sneakers",
    collection: "wearable",
    price: "120",
    credits: null,
    rarity: "epic",
    network: "polygon",
    image: swatch("#7de2ff", "#3b6cff"),
  },
  {
    id: "0x1a2b:1",
    name: "Nebula Flight Jacket",
    collection: "wearable",
    price: "310",
    credits: "45",
    rarity: "legendary",
    network: "polygon",
    image: swatch("#c17bff", "#5f2bd9"),
  },
  {
    id: "0x3c4d:0",
    name: "Solstice Wave",
    collection: "emote",
    price: "35",
    credits: null,
    rarity: "rare",
    network: "polygon",
    image: swatch("#ffd36e", "#ff7a3d"),
  },
  {
    id: "0x5e6f:2",
    name: "Meteor Visor",
    collection: "wearable",
    price: "250",
    credits: null,
    rarity: "mythic",
    network: "ethereum",
    image: swatch("#8affc1", "#1f9d6b"),
  },
];

const gridFixtures = [
  { title: "Genesis Plaza", players: 132, rating: 97, coords: "0,0", creator: "Decentraland Foundation" },
  { title: "Exodus Town", players: 41, rating: 90, coords: "148,60", creator: "Exodus DAO" },
  { title: "Vegas City Plaza", players: 17, rating: 82, coords: "-104,132", creator: "Vegas City" },
];

const meta = {
  title: "Sites Specs/misc/places-shop-entry/PlacesShopEntry",
  component: PlacesShopEntry,
  parameters: {
    layout: "fullscreen",
    a11y: { test: "todo" },
  },
  decorators: [
    (Story) => (
      <div className="pl" style={{ minHeight: "100vh" }}>
        <div className="pl__head">
          <h1 className="pl__title">Places</h1>
        </div>
        <Story />
        <div className="pl__cats">
          {["All", "Art", "Games", "Music"].map((label, i) => (
            <button key={label} type="button" className={"pl__pill" + (i === 0 ? " is-active" : "")}>
              {label}
            </button>
          ))}
        </div>
        <div className="pl__grid">
          {gridFixtures.map((p, i) => (
            <PlaceCard
              key={p.title}
              title={p.title}
              players={p.players}
              rating={p.rating}
              coords={p.coords}
              creator={p.creator}
              hue={(i * 47) % 360}
            />
          ))}
        </div>
      </div>
    ),
  ],
  args: {
    arm: "pill",
    items: railFixtures,
    trackCtx: {
      sid: "sb-spec",
      story: "misc/places-shop-entry",
      variant: "pill",
      experimentKey: "places_shop_entry",
    },
    track: fn(),
    navigate: fn(),
  },
} satisfies Meta<typeof PlacesShopEntry>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Base: Story = {
  args: {
    arm: "base",
    trackCtx: {
      sid: "sb-spec",
      story: "misc/places-shop-entry",
      variant: "base",
      experimentKey: "places_shop_entry",
    },
  },
  play: async ({ args, canvas, canvasElement }) => {
    // Today's surface, untouched: the page paints, no shop entry exists.
    await canvas.findByRole("heading", { name: "Places" });
    await canvas.findByText("Genesis Plaza");
    expect(canvasElement.querySelector(".pl-shop-entry")).toBeNull();
    expect(args.track).not.toHaveBeenCalled();
  },
};

export const Pill: Story = {
  play: async ({ args, canvas }) => {
    const link = await canvas.findByRole("link", { name: "Shop wearables & emotes" });
    await userEvent.click(link);
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith("/shop?from=places-shop-entry"),
    );
    // The shared primary-metric conversion fires on the click-through.
    expect(args.track).toHaveBeenCalledWith(
      "pl_shop_opened",
      expect.objectContaining({ target: "pill", item_id: null, variant: "pill" }),
      expect.anything(),
    );
    expect(args.track).toHaveBeenCalledWith(
      "pl_shop_entry_shown",
      expect.objectContaining({ variant: "pill" }),
      expect.anything(),
    );
  },
};

export const Rail: Story = {
  args: {
    arm: "rail",
    trackCtx: {
      sid: "sb-spec",
      story: "misc/places-shop-entry",
      variant: "rail",
      experimentKey: "places_shop_entry",
    },
  },
  play: async ({ args, canvas }) => {
    await canvas.findByRole("heading", { name: "On sale right now" });
    await canvas.findByText("120 MANA");
    // The credits quote wins over the MANA price when present.
    await canvas.findByText("45 credits");
    await userEvent.click(
      await canvas.findByRole("link", { name: /Aurora Runner Sneakers/ }),
    );
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith(
        "/marketplace/0x1a2b%3A0?from=places-shop-entry",
      ),
    );
    expect(args.track).toHaveBeenCalledWith(
      "pl_shop_opened",
      expect.objectContaining({ target: "rail_item", item_id: "0x1a2b:0", variant: "rail" }),
      expect.anything(),
    );
    await canvas.findByRole("link", { name: "Open the Shop" });
  },
};

// Catalog reading unavailable: say so and keep the CTA -- no invented items,
// never a dead-end.
export const RailUnavailable: Story = {
  args: {
    arm: "rail",
    items: null,
    trackCtx: {
      sid: "sb-spec",
      story: "misc/places-shop-entry",
      variant: "rail",
      experimentKey: "places_shop_entry",
    },
  },
  play: async ({ args, canvas, canvasElement }) => {
    await canvas.findByText("Live listings are unavailable right now.");
    expect(canvasElement.querySelectorAll(".pl-shop-entry li")).toHaveLength(0);
    await userEvent.click(await canvas.findByRole("link", { name: "Open the Shop" }));
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith("/shop?from=places-shop-entry"),
    );
    expect(args.track).toHaveBeenCalledWith(
      "pl_shop_opened",
      expect.objectContaining({ target: "rail_cta", item_id: null, variant: "rail" }),
      expect.anything(),
    );
  },
};
