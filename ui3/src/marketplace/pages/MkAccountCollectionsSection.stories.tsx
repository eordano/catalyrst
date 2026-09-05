import type { Meta, StoryObj } from "@storybook/react-vite";
import type { Collection } from "./MkAccountCollectionsSection";
import MkAccountCollectionsSection from "./MkAccountCollectionsSection";

const SAMPLE_COLLECTIONS: Collection[] = [
  {
    contractAddress: "0x3a1d\u{2026}b7e2",
    name: "Neon District Drop",
    size: 14,
    isOnSale: true,
    tiles: ["legendary", "epic", "rare", "mythic"],
  },
  {
    contractAddress: "0x91cf\u{2026}04ad",
    name: "Cyber Ronin Capsule",
    size: 6,
    isOnSale: true,
    tiles: ["mythic", "legendary"],
  },
  {
    contractAddress: "0x52aa\u{2026}9f81",
    name: "Aurora Wings Collection",
    size: 3,
    isOnSale: false,
    tiles: ["epic", "rare", "uncommon"],
  },
  {
    contractAddress: "0x77c0\u{2026}be12",
    name: "Genesis Plaza Relics",
    size: 1,
    isOnSale: false,
    tiles: ["unique"],
  },
  {
    contractAddress: "0x0b9e\u{2026}42cf",
    name: "Pixel Shades Series",
    size: 24,
    isOnSale: true,
    tiles: ["rare", "uncommon", "common", "epic"],
  },
];

const meta = {
  title: "Marketplace/Pages/Account Collections",
  component: MkAccountCollectionsSection,
  parameters: { layout: "fullscreen" },
  args: {
    collections: SAMPLE_COLLECTIONS,
    count: SAMPLE_COLLECTIONS.length,
  },
} satisfies Meta<typeof MkAccountCollectionsSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const ManyPages: Story = {
  args: {
    count: 38,
  },
};

export const Empty: Story = {
  args: {
    collections: [],
    count: 0,
  },
};

export const Loading: Story = {
  args: {
    isLoading: true,
  },
};
