import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkAccountPage2 from "./MkAccountPage2";

type Props = ComponentProps<typeof MkAccountPage2>;

const SAMPLE_ACCOUNT: NonNullable<Props["account"]> = {
  name: "MetaTraveler",
  address: "0x9f3c2a1b4e7d8c6f0a9b2c3d4e5f6a7b8c9d7a21",
  description:
    "Collector of rare wearables and on-chain art. Building a curated gallery across the metaverse — wander in and browse the showcase.",
  cover: null,
  links: ["website", "twitter", "discord"],
};

const SAMPLE_ITEMS: NonNullable<Props["items"]> = [
  { name: "Cyber Ronin Jacket", collection: "NeonForge", price: "1,250", rarity: "legendary" },
  { name: "Aurora Wings", collection: "Skybound", price: "980", rarity: "mythic" },
  { name: "Golden Crown", collection: "RoyalDCL", price: "3,400", rarity: "unique", network: "ethereum" },
  { name: "Plasma Boots", collection: "NeonForge", price: "640", rarity: "epic" },
  { name: "Sakura Kimono", collection: "EdoStyle", rarity: "rare" },
  { name: "Glitch Mask", collection: "404Wear", price: "199", rarity: "uncommon" },
  { name: "Solar Halo", collection: "Skybound", price: "1,780", rarity: "exotic" },
  { name: "Vapor Tee", collection: "VaporWave", price: "45", rarity: "common" },
  { name: "Dragonscale Cape", collection: "MythMakers", rarity: "legendary" },
  { name: "Holo Backpack", collection: "FutureGear", price: "310", rarity: "epic" },
];

const meta = {
  title: "Marketplace/Pages/Account (other user)",
  component: MkAccountPage2,
  parameters: { layout: "fullscreen" },
  args: {
    account: SAMPLE_ACCOUNT,
    items: SAMPLE_ITEMS,
  },
} satisfies Meta<typeof MkAccountPage2>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Empty: Story = {
  args: { state: "empty", items: [] },
};

export const Loading: Story = {
  args: { state: "loading" },
};

export const Error: Story = {
  args: { state: "error" },
};

export const Guest: Story = {
  args: {
    account: {
      name: "0x742d…9f1c",
      address: "0x742d35cc6634c0532925a3b844bc9e7595f09f1c",
      description: "",
      cover: null,
      links: [],
    },
    items: [
      { name: "Vapor Tee", collection: "VaporWave", price: "45", rarity: "common" },
      { name: "Glitch Mask", collection: "404Wear", price: "199", rarity: "uncommon" },
    ],
  },
};
