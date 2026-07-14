import type { Meta, StoryObj } from "@storybook/react-vite";
import ChItemEditor from "./ChItemEditor";

const COLLECTIONS = [
  { id: "c1", name: "Cyberpunk Streetwear", itemCount: 12, status: "published" },
  { id: "c2", name: "Solarpunk Garden Set", itemCount: 8, status: "draft" },
  { id: "c3", name: "Neon Arcade Capsule", itemCount: 5, status: "under_review" },
  { id: "c4", name: "Genesis Founders Hats", itemCount: 20, status: "published" },
];

const ITEMS = [
  { id: "i1", name: "Holographic Jacket", type: "wearable", rarity: "epic" },
  { id: "i2", name: "LED Visor", type: "wearable", rarity: "rare" },
  { id: "i3", name: "Carbon Sneakers", type: "wearable", rarity: "uncommon" },
  { id: "i4", name: "Reactor Backpack", type: "wearable", rarity: "legendary" },
  { id: "i5", name: "Pulse Gloves", type: "wearable", rarity: "common" },
  { id: "i6", name: "Synthwave Dance", type: "emote", rarity: "rare" },
];

const meta = {
  title: "CreatorHub/Pages/ItemEditor",
  component: ChItemEditor,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ChItemEditor>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    collection: COLLECTIONS[0],
    collections: COLLECTIONS,
    items: ITEMS,
  },
};

export const Empty: Story = {
  args: {},
};

export const ItemsUnavailable: Story = {
  args: {
    collection: COLLECTIONS[1],
    collections: COLLECTIONS,
  },
};
