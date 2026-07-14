import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";
import NewShopHome from "./NewShopHome";
import { banners, featured, rankRows } from "./fixtures";

const meta = {
  title: "Marketplace/NewShop/Pages/Home",
  component: NewShopHome,
  parameters: { layout: "fullscreen" },
  args: {
    activeTab: "overview",
    onTab: fn(),
    banners,
    featured,
    rankTitle: "Top Assets",
    rankRows,
    onOpenAsset: fn(),
    onBuyAsset: fn(),
    onViewAll: fn(),
    onBannerCta: fn(),
    onOpenRank: fn(),
  },
} satisfies Meta<typeof NewShopHome>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WithFavorites: Story = { args: { initialFavorites: ["asset-0", "asset-2"] } };
