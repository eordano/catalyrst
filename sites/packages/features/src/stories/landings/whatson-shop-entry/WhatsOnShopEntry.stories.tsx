// Storybook-only fixtures: the live fetchCatalog loader does not run in
// Storybook, so these feed representative display data. Production stays on
// the real loaders + schema-honesty (see the whats-on route loader).
import type { Meta, StoryObj } from "@ui/docs/sb";
import { expect, fn, userEvent, waitFor } from "@ui/docs/sb";

import "@ui/landings/pages/ldwhatsonpage.css";

import type { CollectibleCard } from "@data/lib/catalyst/marketplace/index";
import WhatsOnShopEntry from "./WhatsOnShopEntry";

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
    name: "Starlight Twirl",
    collection: "emote",
    price: "45",
    credits: null,
    rarity: "epic",
    network: "polygon",
    image: swatch("#7de2ff", "#3b6cff"),
  },
  {
    id: "0x1a2b:1",
    name: "Comet Shuffle",
    collection: "emote",
    price: "80",
    credits: "12",
    rarity: "legendary",
    network: "polygon",
    image: swatch("#c17bff", "#5f2bd9"),
  },
  {
    id: "0x3c4d:0",
    name: "Tidal Groove",
    collection: "emote",
    price: "30",
    credits: null,
    rarity: "rare",
    network: "polygon",
    image: swatch("#ffd36e", "#ff7a3d"),
  },
  {
    id: "0x5e6f:2",
    name: "Ember Waltz",
    collection: "emote",
    price: "120",
    credits: null,
    rarity: "mythic",
    network: "ethereum",
    image: swatch("#8affc1", "#1f9d6b"),
  },
];

const linkFixtures = [
  { id: "ev-1", name: "Neon Rooftop Concert", live: true },
  { id: "ev-2", name: "Museum District Gallery Night", live: false },
  { id: "ev-3", name: "Weekly Builders Meetup", live: false },
];

const meta = {
  title: "Sites Specs/landings/whatson-shop-entry/WhatsOnShopEntry",
  component: WhatsOnShopEntry,
  parameters: {
    layout: "fullscreen",
    a11y: { test: "todo" },
  },
  decorators: [
    (Story) => (
      <div
        className="whatson-route"
        style={{ minHeight: "100vh", background: "#0b0d17", padding: 24 }}
      >
        <div className="whatson-route__cats" role="tablist" aria-label="Event filters">
          {["All", "Today", "This week", "Recurring"].map((label, i) => (
            <button
              key={label}
              type="button"
              role="tab"
              aria-selected={i === 0}
              className={"whatson-route__pill" + (i === 0 ? " is-active" : "")}
            >
              {label}
            </button>
          ))}
        </div>
        <Story />
        <nav className="whatson-route__links" aria-label="All events">
          {linkFixtures.map((link) => (
            <a
              key={link.id}
              href={`/whats-on/${link.id}`}
              className="whatson-route__link"
            >
              {link.name}
              {link.live ? " -- LIVE" : ""}
            </a>
          ))}
        </nav>
      </div>
    ),
  ],
  args: {
    arm: "pill",
    items: railFixtures,
    trackCtx: {
      sid: "sb-spec",
      story: "landings/whatson-shop-entry",
      variant: "pill",
      experimentKey: "lp_whatson_shop_entry",
    },
    track: fn(),
    navigate: fn(),
  },
} satisfies Meta<typeof WhatsOnShopEntry>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Base: Story = {
  args: {
    arm: "base",
    trackCtx: {
      sid: "sb-spec",
      story: "landings/whatson-shop-entry",
      variant: "base",
      experimentKey: "lp_whatson_shop_entry",
    },
  },
  play: async ({ args, canvas, canvasElement }) => {
    // Today's surface, untouched: the schedule chrome paints, no shop entry.
    await canvas.findByRole("tab", { name: "All" });
    await canvas.findByText("Neon Rooftop Concert -- LIVE");
    expect(canvasElement.querySelector(".wo-shop-entry")).toBeNull();
    expect(args.track).not.toHaveBeenCalled();
  },
};

export const Pill: Story = {
  play: async ({ args, canvas }) => {
    const link = await canvas.findByRole("link", {
      name: "Dress for tonight -- Shop emotes & wearables",
    });
    await userEvent.click(link);
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith("/shop?from=whatson-shop-entry"),
    );
    // The shared primary-metric conversion fires on the click-through.
    expect(args.track).toHaveBeenCalledWith(
      "lp_whatson_shop_opened",
      expect.objectContaining({ target: "pill", item_id: null, variant: "pill" }),
      expect.anything(),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_whatson_shop_entry_shown",
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
      story: "landings/whatson-shop-entry",
      variant: "rail",
      experimentKey: "lp_whatson_shop_entry",
    },
  },
  play: async ({ args, canvas }) => {
    await canvas.findByRole("heading", { name: "Fresh emotes on sale" });
    await canvas.findByText("45 MANA");
    // The credits quote wins over the MANA price when present.
    await canvas.findByText("12 credits");
    await userEvent.click(
      await canvas.findByRole("link", { name: /Starlight Twirl/ }),
    );
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith(
        "/marketplace/0x1a2b%3A0?from=whatson-shop-entry",
      ),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_whatson_shop_opened",
      expect.objectContaining({ target: "rail_item", item_id: "0x1a2b:0", variant: "rail" }),
      expect.anything(),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_whatson_shop_entry_shown",
      expect.objectContaining({ variant: "rail" }),
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
      story: "landings/whatson-shop-entry",
      variant: "rail",
      experimentKey: "lp_whatson_shop_entry",
    },
  },
  play: async ({ args, canvas, canvasElement }) => {
    await canvas.findByText("Live listings are unavailable right now.");
    expect(canvasElement.querySelectorAll(".wo-shop-entry li")).toHaveLength(0);
    await userEvent.click(await canvas.findByRole("link", { name: "Open the Shop" }));
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith("/shop?from=whatson-shop-entry"),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_whatson_shop_opened",
      expect.objectContaining({ target: "rail_cta", item_id: null, variant: "rail" }),
      expect.anything(),
    );
  },
};
