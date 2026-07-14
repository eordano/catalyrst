import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkAccountPage from "./MkAccountPage";

type Props = ComponentProps<typeof MkAccountPage>;

const SAMPLE_OWNED: NonNullable<Props["owned"]> = [
  { name: "Cyber Ronin Jacket", collection: "NeonForge", price: "1,250", rarity: "legendary" },
  { name: "Aurora Wings", collection: "Skybound", rarity: "mythic" },
  { name: "Pixel Shades", collection: "8bitClub", price: "120", rarity: "rare" },
  { name: "Golden Crown", collection: "RoyalDCL", rarity: "unique", network: "ethereum" },
  { name: "Frost Hoodie", collection: "WinterSet", price: "85", rarity: "uncommon" },
  { name: "Plasma Boots", collection: "NeonForge", rarity: "epic", network: "ethereum" },
  { name: "Vapor Tee", collection: "VaporWave", price: "45", rarity: "common" },
  { name: "Dragonscale Cape", collection: "MythMakers", rarity: "legendary" },
  { name: "Holo Backpack", collection: "FutureGear", price: "310", rarity: "epic" },
  { name: "Sakura Kimono", collection: "EdoStyle", rarity: "rare", network: "ethereum" },
  { name: "Glitch Mask", collection: "404Wear", price: "199", rarity: "uncommon" },
  { name: "Solar Halo", collection: "Skybound", price: "1,780", rarity: "exotic" },
];

const SAMPLE_ON_SALE: NonNullable<Props["onSale"]> = [
  { name: "Cyber Ronin Jacket", collection: "NeonForge", rarity: "legendary", saleType: "Listing", price: "1,250", expiresIn: "in 28 days" },
  { name: "Pixel Shades", collection: "8bitClub", rarity: "rare", saleType: "Listing", price: "120", expiresIn: "in 12 days" },
  { name: "Solar Halo", collection: "Skybound", rarity: "exotic", saleType: "Listing", price: "1,780", expiresIn: "in 5 days" },
  { name: "Glitch Mask", collection: "404Wear", rarity: "uncommon", saleType: "Listing", price: "199", expiresIn: "in 30 days" },
];

const SAMPLE_SALES: NonNullable<Props["sales"]> = [
  { name: "Frost Hoodie", rarity: "uncommon", type: "Sale", from: "0x9f3c…7a21", to: "0x1ab4…0d3e", price: "85", date: "Jun 12, 2026" },
  { name: "Vapor Tee", rarity: "common", type: "Sale", from: "0x9f3c…7a21", to: "0x77c0…be12", price: "45", date: "Jun 04, 2026" },
  { name: "Holo Backpack", rarity: "epic", type: "Sale", from: "0x9f3c…7a21", to: "0x52aa…9f81", price: "310", date: "May 28, 2026" },
];

const meta = {
  title: "Marketplace/Pages/Account (My Assets)",
  component: MkAccountPage,
  parameters: { layout: "fullscreen" },
  args: {
    owned: SAMPLE_OWNED,
    onSale: SAMPLE_ON_SALE,
    sales: SAMPLE_SALES,
  },
} satisfies Meta<typeof MkAccountPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const OnSale: Story = {
  args: { initialSection: "on_sale" },
};

export const Sales: Story = {
  args: { initialSection: "sales" },
};

export const Collections: Story = {
  args: { initialSection: "collections" },
};

export const Empty: Story = {
  args: { owned: [] },
};
