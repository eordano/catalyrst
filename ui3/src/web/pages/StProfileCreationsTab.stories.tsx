import { siteUrl } from "../../data/site";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StProfileCreationsTab from "./StProfileCreationsTab";
import type { CreationItem, CreationProfile } from "./StProfileCreationsTab";

const PROFILE: CreationProfile = {
  address: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
  name: "PixelNomad",
  accountUrl: siteUrl("/shop"),
};

const WEARABLES: CreationItem[] = [
  { id: "w1", name: "Cyber Halo", creator: "PixelNomad", price: "350", rarity: "epic", category: "hat", body: "unisex", smart: false },
  { id: "w2", name: "Aurora Jacket", creator: "PixelNomad", price: "1,200", rarity: "legendary", category: "upper_body", body: "female", smart: false },
  { id: "w3", name: "Glitch Sneakers", creator: "PixelNomad", price: "85", rarity: "rare", category: "feet", body: "unisex", smart: false },
  { id: "w4", name: "Founders Crown", creator: "PixelNomad", price: null, rarity: "mythic", category: "tiara", body: "unisex", smart: false },
  { id: "w5", name: "Neon Visor", creator: "PixelNomad", price: "640", rarity: "epic", category: "eyewear", body: "male", smart: true },
  { id: "w6", name: "Polar Mittens", creator: "PixelNomad", price: "42", rarity: "uncommon", category: "hands_wear", body: "unisex", smart: false },
  { id: "w7", name: "Static Mask", creator: "PixelNomad", price: null, rarity: "unique", category: "mask", body: "unisex", smart: false },
  { id: "w8", name: "Plasma Trousers", creator: "PixelNomad", price: "210", rarity: "rare", category: "lower_body", body: "male", smart: false },
];

const EMOTES: CreationItem[] = [
  { id: "e1", name: "Hover Spin", creator: "PixelNomad", price: "120", rarity: "epic", category: null, body: null, smart: false },
  { id: "e2", name: "Pixel Bow", creator: "PixelNomad", price: "60", rarity: "rare", category: null, body: null, smart: false },
  { id: "e3", name: "Glitch Wave", creator: "PixelNomad", price: null, rarity: "legendary", category: null, body: null, smart: false },
  { id: "e4", name: "Synth Step", creator: "PixelNomad", price: "95", rarity: "uncommon", category: null, body: null, smart: false },
];

const meta = {
  title: "Web/Pages/Profile/Creations Tab",
  component: StProfileCreationsTab,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof StProfileCreationsTab>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { profile: PROFILE, wearables: WEARABLES, emotes: EMOTES },
};

export const Empty: Story = {
  args: { profile: PROFILE, empty: true },
};

export const Loading: Story = {
  args: { profile: PROFILE, loading: true },
};
