import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkMyBids from "./MkMyBids";

type Props = ComponentProps<typeof MkMyBids>;

const SAMPLE_SELLER_BIDS: NonNullable<Props["sellerBids"]> = [
  {
    id: "s1",
    name: "Cyber Ronin Jacket",
    bidder: "NeonNomad",
    hue: 268,
    tile: "var(--rar-legendary)",
    created: "2 days ago",
    price: "1,250",
    timeLeft: "6 days",
  },
  {
    id: "s2",
    name: "Aurora Wings",
    bidder: "0x9f3c…7a21",
    hue: 200,
    tile: "var(--rar-mythic)",
    created: "5 hours ago",
    price: "980",
    timeLeft: "2 days",
  },
];

const SAMPLE_ARCHIVED_BIDS: NonNullable<Props["archivedBids"]> = [
  {
    id: "a1",
    name: "Pixel Shades",
    bidder: "RetroByte",
    hue: 120,
    tile: "var(--rar-rare)",
    created: "3 weeks ago",
    price: "120",
    timeLeft: "expired",
  },
];

const SAMPLE_BIDDER_BIDS: NonNullable<Props["bidderBids"]> = [
  {
    id: "b1",
    name: "Golden Crown",
    bidder: "You",
    hue: 40,
    tile: "var(--rar-unique)",
    created: "1 day ago",
    price: "3,400",
    timeLeft: "12 days",
    warning: null,
  },
  {
    id: "b2",
    name: "Plasma Boots",
    bidder: "You",
    hue: 290,
    tile: "var(--rar-epic)",
    created: "4 days ago",
    price: "640",
    timeLeft: "3 days",
    warning: "You don't have enough MANA to pay for this bid",
  },
];

const meta = {
  title: "Marketplace/Pages/My Bids",
  component: MkMyBids,
  parameters: { layout: "fullscreen" },
  args: {
    sellerBids: SAMPLE_SELLER_BIDS,
    archivedBids: SAMPLE_ARCHIVED_BIDS,
    bidderBids: SAMPLE_BIDDER_BIDS,
  },
} satisfies Meta<typeof MkMyBids>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Loading: Story = {
  args: {
    isLoading: true,
    sellerBids: [],
    archivedBids: [],
    bidderBids: [],
  },
};

export const Empty: Story = {
  args: {
    isLoading: false,
    sellerBids: [],
    archivedBids: [],
    bidderBids: [],
  },
};

export const Connecting: Story = {
  args: { isConnecting: true },
};
