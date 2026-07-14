import type { ComponentProps, ReactNode } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import AssetCard from "./AssetCard";

type CardProps = ComponentProps<typeof AssetCard>;

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

const Frame = ({ children }: { children?: ReactNode }) => <div style={{ width: 220 }}>{children}</div>;

const meta = {
  title: "Marketplace/Components/AssetCard",
  component: AssetCard,
  parameters: {
    layout: "centered",
    backgrounds: { default: "market", values: [{ name: "market", value: "#0e0d10" }] },
  },
  argTypes: {
    name: { control: "text" },
    collection: { control: "text" },
    price: { control: "text" },
    unit: { control: "inline-radio", options: ["mana", "credits"] },
    rarity: { control: "select", options: RARITIES },
    network: { control: "inline-radio", options: ["polygon", "ethereum"] },
    tag: { control: "text" },
  },
  args: {
    name: "Cyber Ronin Jacket",
    collection: "NeonForge",
    price: "1,250",
    rarity: "legendary",
    tag: "Mint",
  },
  render: (args) => (
    <Frame>
      <AssetCard {...args} />
    </Frame>
  ),
} satisfies Meta<typeof AssetCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CASES: { label: string; args: Partial<CardProps> }[] = [
  ...RARITIES.map((rarity) => ({
    label: rarity,
    args: {
      name: rarity.charAt(0).toUpperCase() + rarity.slice(1),
      collection: "Sample Set",
      price: "500",
      rarity,
      tag: undefined,
    },
  })),
  {
    label: "Not for sale",
    args: {
      name: "Aurora Wings",
      collection: "Skybound",
      rarity: "mythic",
      price: undefined,
      tag: undefined,
    },
  },
];

/**
 * Every rarity plus the not-for-sale card. `Default` flips between them from the Controls panel;
 * this keeps all of them in the render + a11y + visual-diff gates.
 */
export const Catalog: Story = {
  name: "Catalog (every rarity)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", gap: 16, flexWrap: "wrap", width: 760 }}>
      {CASES.map((c) => (
        <section key={c.label} style={{ width: 170 }}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <AssetCard {...args} {...c.args} />
        </section>
      ))}
    </div>
  ),
};
