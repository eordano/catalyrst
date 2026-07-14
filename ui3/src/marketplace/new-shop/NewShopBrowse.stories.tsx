import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";
import NewShopBrowse from "./NewShopBrowse";
import { filterGroups, makeCards } from "./fixtures";

const meta = {
  title: "Marketplace/NewShop/Pages/Browse",
  component: NewShopBrowse,
  parameters: { layout: "fullscreen" },
  args: {
    activeTab: "all-assets",
    onTab: fn(),
    groups: filterGroups,
    cards: makeCards(12),
    itemCount: "5,373 items",
    onSort: fn(),
    onToggleOnSale: fn(),
    onOptionChange: fn(),
    onOpenAsset: fn(),
    onBuyAsset: fn(),
  },
} satisfies Meta<typeof NewShopBrowse>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Empty: Story = { args: { cards: [] } };
