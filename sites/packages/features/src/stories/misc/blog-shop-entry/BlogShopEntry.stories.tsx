// Storybook-only fixtures: the live fetchCatalog loader does not run in
// Storybook, so these feed representative display data. Production stays on
// the real loaders + schema-honesty (see the blog index route loader).
import type { Meta, StoryObj } from "@ui/docs/sb";
import { expect, fn, userEvent, waitFor } from "@ui/docs/sb";

import "@ui/web/pages/stbloghome.css";

import type { CollectibleCard } from "@data/lib/catalyst/marketplace/index";
import BlogShopEntry from "./BlogShopEntry";

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
    id: "0x7a8b:0",
    name: "Comet Trail Hoodie",
    collection: "wearable",
    price: "140",
    credits: null,
    rarity: "epic",
    network: "polygon",
    image: swatch("#7de2ff", "#3b6cff"),
  },
  {
    id: "0x7a8b:1",
    name: "Tidal Glass Visor",
    collection: "wearable",
    price: "290",
    credits: "40",
    rarity: "legendary",
    network: "polygon",
    image: swatch("#c17bff", "#5f2bd9"),
  },
  {
    id: "0x9c0d:0",
    name: "Ember Step",
    collection: "emote",
    price: "30",
    credits: null,
    rarity: "rare",
    network: "polygon",
    image: swatch("#ffd36e", "#ff7a3d"),
  },
  {
    id: "0x1e2f:3",
    name: "Orbit Weave Cape",
    collection: "wearable",
    price: "220",
    credits: null,
    rarity: "mythic",
    network: "ethereum",
    image: swatch("#8affc1", "#1f9d6b"),
  },
];

const postFixtures = [
  {
    id: "platform-update-q3",
    title: "Platform update: what shipped this quarter",
    date: "Aug 18, 2026",
    category: "Product",
    hue: 210,
  },
  {
    id: "creator-spotlight-gardens",
    title: "Creator spotlight: building Terrace Gardens",
    date: "Aug 12, 2026",
    category: "Creators",
    hue: 120,
  },
  {
    id: "music-week-recap",
    title: "Music week recap: five stages, one weekend",
    date: "Aug 5, 2026",
    category: "Events",
    hue: 20,
  },
];

const meta = {
  title: "Sites Specs/misc/blog-shop-entry/BlogShopEntry",
  component: BlogShopEntry,
  parameters: {
    layout: "fullscreen",
    a11y: { test: "todo" },
  },
  decorators: [
    (Story) => (
      <div
        className="blog-index-route"
        style={{ minHeight: "100vh", background: "#17141c" }}
      >
        <Story />
        <div className="stbloghome">
          <div className="stbloghome__nav">
            <div className="stbloghome__navcontent">
              <div className="stbloghome__navwrap">
                <nav className="stbloghome__cats" aria-label="Blog categories">
                  <ul className="stbloghome__catlist">
                    {["All", "Product", "Creators", "Events"].map((label, i) => (
                      <li className="stbloghome__catitem" key={label}>
                        <a
                          className={"stbloghome__catlink" + (i === 0 ? " is-active" : "")}
                          href="/blog"
                        >
                          {label}
                        </a>
                      </li>
                    ))}
                  </ul>
                </nav>
              </div>
            </div>
          </div>
          <div className="stbloghome__content">
            <div className="stbloghome__list">
              {postFixtures.map((p) => (
                <article className="stbloghome__card" key={p.id}>
                  <a className="stbloghome__cardimglink" href={`/blog/${p.id}`}>
                    <span
                      className="stbloghome__cardimg"
                      role="img"
                      aria-label={p.title}
                      style={{
                        backgroundImage: `linear-gradient(150deg, hsl(${p.hue} 70% 52%) 0%, hsl(${(p.hue + 40) % 360} 60% 28%) 100%)`,
                      }}
                    />
                  </a>
                  <div className="stbloghome__cardinfo">
                    <div className="stbloghome__meta">
                      <span className="stbloghome__date">{p.date}</span>
                      <span>
                        <a className="stbloghome__catlnk" href="/blog">
                          {p.category}
                        </a>
                      </span>
                    </div>
                    <a className="stbloghome__titlelink" href={`/blog/${p.id}`}>
                      <h2 className="stbloghome__cardtitle">{p.title}</h2>
                    </a>
                  </div>
                </article>
              ))}
            </div>
          </div>
        </div>
      </div>
    ),
  ],
  args: {
    arm: "card",
    items: railFixtures,
    trackCtx: {
      sid: "sb-spec",
      story: "misc/blog-shop-entry",
      variant: "card",
      experimentKey: "lp_blog_shop_entry",
    },
    track: fn(),
    navigate: fn(),
  },
} satisfies Meta<typeof BlogShopEntry>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Base: Story = {
  args: {
    arm: "base",
    trackCtx: {
      sid: "sb-spec",
      story: "misc/blog-shop-entry",
      variant: "base",
      experimentKey: "lp_blog_shop_entry",
    },
  },
  play: async ({ args, canvas, canvasElement }) => {
    // Today's surface, untouched: the post grid paints, no shop entry exists.
    await canvas.findByText("Platform update: what shipped this quarter");
    expect(canvasElement.querySelector(".blog-shop-entry")).toBeNull();
    expect(args.track).not.toHaveBeenCalled();
  },
};

export const Card: Story = {
  play: async ({ args, canvas }) => {
    const link = await canvas.findByRole("link", { name: /Shop the latest/ });
    await userEvent.click(link);
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith("/shop?from=blog-shop-entry"),
    );
    // The shared primary-metric conversion fires on the click-through.
    expect(args.track).toHaveBeenCalledWith(
      "lp_blog_shop_opened",
      expect.objectContaining({ target: "card", item_id: null, variant: "card" }),
      expect.anything(),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_blog_shop_entry_shown",
      expect.objectContaining({ variant: "card" }),
      expect.anything(),
    );
  },
};

export const Rail: Story = {
  args: {
    arm: "rail",
    trackCtx: {
      sid: "sb-spec",
      story: "misc/blog-shop-entry",
      variant: "rail",
      experimentKey: "lp_blog_shop_entry",
    },
  },
  play: async ({ args, canvas }) => {
    await canvas.findByRole("heading", { name: "Fresh drops on sale" });
    await canvas.findByText("140 MANA");
    // The credits quote wins over the MANA price when present.
    await canvas.findByText("40 credits");
    await userEvent.click(
      await canvas.findByRole("link", { name: /Comet Trail Hoodie/ }),
    );
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith(
        "/marketplace/0x7a8b%3A0?from=blog-shop-entry",
      ),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_blog_shop_opened",
      expect.objectContaining({ target: "rail_item", item_id: "0x7a8b:0", variant: "rail" }),
      expect.anything(),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_blog_shop_entry_shown",
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
      story: "misc/blog-shop-entry",
      variant: "rail",
      experimentKey: "lp_blog_shop_entry",
    },
  },
  play: async ({ args, canvas, canvasElement }) => {
    await canvas.findByText("Live listings are unavailable right now.");
    expect(canvasElement.querySelectorAll(".blog-shop-entry li")).toHaveLength(0);
    await userEvent.click(await canvas.findByRole("link", { name: "Open the Shop" }));
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith("/shop?from=blog-shop-entry"),
    );
    expect(args.track).toHaveBeenCalledWith(
      "lp_blog_shop_opened",
      expect.objectContaining({ target: "rail_cta", item_id: null, variant: "rail" }),
      expect.anything(),
    );
  },
};
