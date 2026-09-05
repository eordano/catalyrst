import type { Meta, StoryObj } from "@storybook/react-vite";
import MkBidPage from "./MkBidPage";

const SAMPLE_NFT = {
  name: "Cyber Ronin Jacket",
  category: "wearable",
  rarity: "legendary",
  network: "polygon",
};

const meta = {
  title: "Marketplace/Pages/Bid",
  component: MkBidPage,
  parameters: { layout: "fullscreen" },
  args: {
    nft: SAMPLE_NFT,
    manaBalance: 50000,
  },
} satisfies Meta<typeof MkBidPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {},
};

export const WithPrice: Story = {
  args: {
    initialPrice: "1,250",
    initialExpiration: "2026-08-15",
    nft: {
      name: "Aurora Wings",
      category: "wearable",
      rarity: "mythic",
      network: "polygon",
    },
  },
};

export const InsufficientMana: Story = {
  args: {
    initialPrice: "5,000",
    manaBalance: 1200,
    nft: {
      name: "Golden Crown",
      category: "wearable",
      rarity: "unique",
      network: "ethereum",
    },
  },
};

export const PriceTooLow: Story = {
  args: {
    initialPrice: "0.5",
    notOnPolygon: true,
  },
};

export const Confirm: Story = {
  args: {
    initialPrice: "980",
    initialStage: "confirm",
  },
};

export const Authorization: Story = {
  args: {
    initialPrice: "980",
    needsAuthorization: true,
    initialStage: "authorize",
  },
};

export const Placing: Story = {
  args: {
    initialPrice: "980",
    initialStage: "confirm",
    isPlacingBid: true,
  },
};

export const InvalidDate: Story = {
  args: {
    initialPrice: "560",
    initialExpiration: "2020-01-01",
  },
};
