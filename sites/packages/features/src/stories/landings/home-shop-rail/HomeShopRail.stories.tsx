// Storybook-only fixtures: the live fetchCatalog loader does not run in
// Storybook, so these feed representative display data. Production stays on
// the real loaders + schema-honesty (see the landings.home route loader).
import type { Meta, StoryObj } from "@ui/docs/sb";
import { expect, fn, userEvent, waitFor } from "@ui/docs/sb";

import "@ui/landings/pages/ldhomepage.css";

import type { CollectibleCard } from "@data/lib/catalyst/marketplace/index";
import HomeShopRail from "./HomeShopRail";

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
    id: "0x9a01:0",
    name: "Comet Trail Hoodie",
    collection: "wearable",
    price: "95",
    credits: null,
    rarity: "epic",
    network: "polygon",
    image: swatch("#7de2ff", "#3b6cff"),
  },
  {
    id: "0x9a01:3",
    name: "Lunar Drift Kicks",
    collection: "wearable",
    price: "180",
    credits: "30",
    rarity: "legendary",
    network: "polygon",
    image: swatch("#c17bff", "#5f2bd9"),
  },
  {
    id: "0x2b77:1",
    name: "Tidal Spin",
    collection: "emote",
    price: "28",
    credits: null,
    rarity: "rare",
    network: "polygon",
    image: swatch("#8affc1", "#1f9d6b"),
  },
  {
    id: "0x2b77:4",
    name: "Ember Crown",
    collection: "wearable",
    price: "240",
    credits: null,
    rarity: "mythic",
    network: "ethereum",
    image: swatch("#ffd36e", "#ff7a3d"),
  },
  {
    id: "0x5c19:2",
    name: "Prism Visor",
    collection: "wearable",
    price: "60",
    credits: null,
    rarity: "epic",
    network: "polygon",
    image: swatch("#ff9ad5", "#d9418f"),
  },
  {
    id: "0x5c19:5",
    name: "Static Bloom",
    collection: "emote",
    price: "40",
    credits: null,
    rarity: "rare",
    network: "polygon",
    image: swatch("#b8c6ff", "#4653c9"),
  },
];

const meta = {
  title: "Sites Specs/landings/home-shop-rail/HomeShopRail",
  component: HomeShopRail,
  parameters: {
    layout: "fullscreen",
    a11y: { test: "todo" },
  },
  decorators: [
    (Story) => (
      <div className="landings-home" style={{ minHeight: "100vh" }}>
        <section
          id="hero"
          aria-label="Jump in to Decentraland"
          className="landings-home__cta"
        >
          <h2 className="landings-home__kicker">Explore together</h2>
          <p className="landings-home__title">Your world, your rules</p>
          <p className="landings-home__sub">
            Create, explore and trade in the first virtual world owned by its
            users.
          </p>
          <div id="download-cta" className="landings-home__btns">
            <a
              className="landings-home__btn landings-home__btn--primary"
              href="#download-cta"
              onClick={(e) => e.preventDefault()}
            >
              Download for desktop
            </a>
            <a
              className="landings-home__btn"
              href="#download-cta"
              onClick={(e) => e.preventDefault()}
            >
              Get it on Epic Games
            </a>
          </div>
        </section>
        <Story />
        <section
          id="feature-rails"
          aria-label="Explore Decentraland"
          className="landings-home__rails"
        >
          <div className="landings-home__rail" aria-label="Whats on">
            <div className="landings-home__railhead">
              <h3 className="landings-home__railtitle">{"What's On"}</h3>
            </div>
            <nav className="landings-home__raillinks" aria-label="Whats on">
              <a
                className="landings-home__raillink"
                href="#feature-rails"
                onClick={(e) => e.preventDefault()}
              >
                <span className="landings-home__railcat">MUSIC</span>
                Rooftop Frequencies
              </a>
              <a
                className="landings-home__raillink"
                href="#feature-rails"
                onClick={(e) => e.preventDefault()}
              >
                <span className="landings-home__railcat">GAMES</span>
                Puzzle Rush Arena
              </a>
            </nav>
          </div>
        </section>
      </div>
    ),
  ],
  args: {
    arm: "cta",
    items: railFixtures,
    trackCtx: {
      sid: "sb-spec",
      story: "landings/home-shop-rail",
      variant: "cta",
      experimentKey: "lp_home_shop_rail",
    },
    track: fn(),
    navigate: fn(),
  },
} satisfies Meta<typeof HomeShopRail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Base: Story = {
  args: {
    arm: "base",
    trackCtx: {
      sid: "sb-spec",
      story: "landings/home-shop-rail",
      variant: "base",
      experimentKey: "lp_home_shop_rail",
    },
  },
  play: async ({ args, canvas, canvasElement }) => {
    // Today's surface, untouched: hero and rails paint, no shop entry exists.
    await canvas.findByText("Your world, your rules");
    await canvas.findByText("Download for desktop");
    await canvas.findByText("Rooftop Frequencies");
    expect(canvasElement.querySelector(".home-shop-rail")).toBeNull();
    expect(args.track).not.toHaveBeenCalled();
  },
};

export const Cta: Story = {
  play: async ({ args, canvas }) => {
    const link = await canvas.findByRole("link", {
      name: "Shop the latest wearable drops",
    });
    await userEvent.click(link);
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith("/shop?from=home-shop-rail"),
    );
    // The shared primary-metric conversion fires on the click-through.
    expect(args.track).toHaveBeenCalledWith(
      "lp_home_shop_opened",
      expect.objectContaining({ target: "cta", item_id: null, variant: "cta" }),
      expect.anything(),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_home_shop_entry_shown",
      expect.objectContaining({ variant: "cta" }),
      expect.anything(),
    );
  },
};

export const Rail: Story = {
  args: {
    arm: "rail",
    trackCtx: {
      sid: "sb-spec",
      story: "landings/home-shop-rail",
      variant: "rail",
      experimentKey: "lp_home_shop_rail",
    },
  },
  play: async ({ args, canvas }) => {
    await canvas.findByRole("heading", { name: "Fresh drops on sale" });
    await canvas.findByText("95 MANA");
    // The credits quote wins over the MANA price when present.
    await canvas.findByText("30 credits");
    await userEvent.click(
      await canvas.findByRole("link", { name: /Comet Trail Hoodie/ }),
    );
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith(
        "/marketplace/0x9a01%3A0?from=home-shop-rail",
      ),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_home_shop_opened",
      expect.objectContaining({
        target: "rail_item",
        item_id: "0x9a01:0",
        variant: "rail",
      }),
      expect.anything(),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_home_shop_entry_shown",
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
      story: "landings/home-shop-rail",
      variant: "rail",
      experimentKey: "lp_home_shop_rail",
    },
  },
  play: async ({ args, canvas, canvasElement }) => {
    await canvas.findByText("Live listings are unavailable right now.");
    expect(canvasElement.querySelectorAll(".home-shop-rail li")).toHaveLength(0);
    await userEvent.click(await canvas.findByRole("link", { name: "Open the Shop" }));
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith("/shop?from=home-shop-rail"),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_home_shop_opened",
      expect.objectContaining({ target: "rail_cta", item_id: null, variant: "rail" }),
      expect.anything(),
    );
  },
};
