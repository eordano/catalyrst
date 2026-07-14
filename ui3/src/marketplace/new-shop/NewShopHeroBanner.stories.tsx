import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";
import NewShopHeroBanner from "./NewShopHeroBanner";

type BannerProps = ComponentProps<typeof NewShopHeroBanner>;

const TONES: NonNullable<BannerProps["tone"]>[] = ["purple", "magenta", "neon"];

const meta = {
  title: "Marketplace/NewShop/HeroBanner",
  component: NewShopHeroBanner,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ maxWidth: 560, background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
  argTypes: {
    tone: { control: "inline-radio", options: TONES },
    eyebrow: { control: "text" },
    title: { control: "text" },
    subtitle: { control: "text" },
    cta: { control: "text" },
  },
  args: {
    eyebrow: "Trending now",
    title: "Best Rated Emotes",
    subtitle: "The community's top-voted moves this week.",
    cta: "Shop emotes",
    tone: "purple",
    onCta: fn(),
  },
} satisfies Meta<typeof NewShopHeroBanner>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Same args as `Default`; kept as its own export because the story id
 * `marketplace-newshop-herobanner--purple` is a screen-tour deep link
 * (`tools/screen-tour/sb-links.mts`).
 */
export const Purple: Story = {};

const CASES: { label: string; args: Partial<BannerProps> }[] = [
  { label: "Purple", args: {} },
  { label: "Magenta", args: { title: "Week Selected Outfits", tone: "magenta" } },
  {
    label: "Neon",
    args: {
      title: "MANA Live",
      subtitle: "Grab wearables from the concert drop.",
      tone: "neon",
    },
  },
  { label: "No art, no CTA", args: { cta: undefined, subtitle: undefined } },
];

/** Every tone plus the trimmed variant, all kept in the render + a11y + visual-diff gates. */
export const Catalog: Story = {
  name: "Catalog (every tone)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      {CASES.map((c) => (
        <section key={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <NewShopHeroBanner {...args} {...c.args} />
        </section>
      ))}
    </div>
  ),
};
