import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";
import NewShopAssetCard from "./NewShopAssetCard";

type CardProps = ComponentProps<typeof NewShopAssetCard>;

const RARITIES = [
  "common",
  "uncommon",
  "rare",
  "epic",
  "legendary",
  "mythic",
  "unique",
  "exotic",
];

const meta = {
  title: "Marketplace/NewShop/AssetCard",
  component: NewShopAssetCard,
  parameters: { layout: "centered" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ width: 220, background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
  argTypes: {
    name: { control: "text" },
    meta: { control: "text" },
    price: { control: "text" },
    unit: { control: "inline-radio", options: ["mana", "credits"] },
    rarity: { control: "select", options: RARITIES },
    network: { control: "inline-radio", options: ["polygon", "ethereum"] },
    favorited: { control: "boolean" },
    buyLabel: { control: "text" },
  },
  args: {
    name: "Golden Sneakers",
    meta: "5d 12h ago",
    price: "500",
    rarity: "legendary",
    network: "polygon",
    favorited: false,
    onToggleFavorite: fn(),
    onOpen: fn(),
    onBuy: fn(),
  },
} satisfies Meta<typeof NewShopAssetCard>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CASES: { label: string; args: Partial<CardProps> }[] = [
  ...RARITIES.map((rarity) => ({ label: rarity, args: { name: rarity, rarity } })),
  { label: "Favorited", args: { favorited: true } },
  { label: "Not for sale", args: { price: undefined, rarity: "rare", onBuy: undefined } },
  {
    label: "Ethereum",
    args: { name: "Vintage Hat", rarity: "epic", network: "ethereum", price: "1,250" },
  },
];

/**
 * Every rarity plus the favorited / not-for-sale / Ethereum variants. `Default` flips between
 * them from the Controls panel; this keeps all of them in the render + a11y + visual-diff gates.
 */
export const Catalog: Story = {
  name: "Catalog (every rarity + state)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div
      className="mk"
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(4, 200px)",
        gap: 16,
        background: "var(--lm-bg)",
        padding: 16,
      }}
    >
      {CASES.map((c) => (
        <section key={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <NewShopAssetCard {...args} {...c.args} />
        </section>
      ))}
    </div>
  ),
};
