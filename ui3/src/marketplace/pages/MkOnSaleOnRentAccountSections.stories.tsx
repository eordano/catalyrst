import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkOnSaleOnRentAccountSections from "./MkOnSaleOnRentAccountSections";

type Props = ComponentProps<typeof MkOnSaleOnRentAccountSections>;

const SAMPLE_ON_SALE: NonNullable<Props["onSale"]> = [
  { id: "n1", name: "Cyber Ronin Jacket", sub: "", category: "wearable", rarity: "legendary", saleType: "secondary", price: "1,250" },
  { id: "n2", name: "Genesis Plaza Parcel", sub: "-42,18", category: "parcel", rarity: "rare", saleType: "secondary", price: "9,400" },
  { id: "n3", name: "Pixel Shades", sub: "", category: "wearable", rarity: "rare", saleType: "primary", price: "120" },
  { id: "n4", name: "Solar Halo", sub: "", category: "emote", rarity: "exotic", saleType: "secondary", price: "1,780", needsAttention: true },
  { id: "n5", name: "Aetheria Estate", sub: "6 parcels", category: "estate", rarity: "epic", saleType: "secondary", price: "21,000" },
  { id: "n6", name: "Glitch Mask", sub: "", category: "wearable", rarity: "uncommon", saleType: "secondary", price: "199", legacyExpired: true },
  { id: "n7", name: "frostfang", sub: "DCL Name", category: "ens", rarity: "unique", saleType: "secondary", price: "2,000" },
  { id: "n8", name: "Holo Backpack", sub: "", category: "wearable", rarity: "epic", saleType: "primary", price: "310" },
  { id: "n9", name: "Vapor Tee", sub: "", category: "wearable", rarity: "common", saleType: "secondary", price: "45", legacy: true },
  { id: "n10", name: "Dragonscale Cape", sub: "", category: "wearable", rarity: "legendary", saleType: "secondary", price: "640" },
  { id: "n11", name: "Sakura Kimono", sub: "", category: "wearable", rarity: "rare", saleType: "secondary", price: "275" },
  { id: "n12", name: "Plasma Boots", sub: "", category: "wearable", rarity: "epic", saleType: "primary", price: "180" },
  { id: "n13", name: "Neon District Parcel", sub: "12,-7", category: "parcel", rarity: "rare", saleType: "secondary", price: "8,900" },
];

const SAMPLE_ON_RENT: NonNullable<Props["onRent"]> = [
  { id: "r1", name: "Aetheria Estate", sub: "6 parcels", category: "estate", rarity: "epic", status: "open", price: "120" },
  { id: "r2", name: "Genesis Plaza Parcel", sub: "-42,18", category: "parcel", rarity: "rare", status: "rented", endDate: "Jul 14", price: "45" },
  { id: "r3", name: "Neon District Parcel", sub: "12,-7", category: "parcel", rarity: "rare", status: "open", price: "60" },
  { id: "r4", name: "Riverside Estate", sub: "3 parcels", category: "estate", rarity: "legendary", status: "over", price: "90" },
  { id: "r5", name: "Skyline Parcel", sub: "88,4", category: "parcel", rarity: "common", status: "claiming", price: "30" },
];

const meta = {
  title: "Marketplace/Pages/On Sale / On Rent",
  component: MkOnSaleOnRentAccountSections,
  parameters: { layout: "fullscreen" },
  argTypes: {
    type: { control: "inline-radio", options: ["sale", "rent"] },
    isEmpty: { control: "boolean" },
    isLoading: { control: "boolean" },
  },
  args: {
    onSale: SAMPLE_ON_SALE,
    onRent: SAMPLE_ON_RENT,
    type: "sale",
    isEmpty: false,
    isLoading: false,
  },
} satisfies Meta<typeof MkOnSaleOnRentAccountSections>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CASES: { label: string; args: Partial<Props> }[] = [
  { label: "On sale", args: { type: "sale" } },
  { label: "On rent", args: { type: "rent" } },
  { label: "Empty", args: { type: "sale", isEmpty: true } },
  { label: "Loading", args: { type: "sale", isLoading: true } },
];

/**
 * Every state at once. `Default` flips between them from the Controls panel; this keeps all four
 * in the render + a11y + visual-diff gates, since the rent table, the empty state and the loading
 * skeleton are structurally different subtrees. `chrome={false}` because stacking N copies of
 * `MarketplaceChrome` would emit N `<main>` landmarks and fail axe's landmark-unique.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CASES.map((c) => (
        <section key={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <MkOnSaleOnRentAccountSections {...args} {...c.args} chrome={false} />
        </section>
      ))}
    </div>
  ),
};
