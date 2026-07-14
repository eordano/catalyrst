import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, fn, userEvent, within } from "storybook/test";
import NewShopAssetCard from "./NewShopAssetCard";

const meta = {
  title: "Marketplace/NewShop/AssetCard/Interactions",
  component: NewShopAssetCard,
  parameters: { layout: "centered" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ width: 220, background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
} satisfies Meta<typeof NewShopAssetCard>;
export default meta;

type Story = StoryObj<typeof meta>;

export const FavoriteToggles: Story = {
  render: (args) => {
    const [on, setOn] = useState(false);
    return (
      <NewShopAssetCard
        {...args}
        favorited={on}
        onToggleFavorite={() => {
          setOn((v) => !v);
          args.onToggleFavorite?.();
        }}
      />
    );
  },
  args: { name: "Cyber Cap", price: "500", rarity: "epic", onToggleFavorite: fn(), onOpen: fn(), onBuy: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const heart = canvas.getByRole("button", { name: "Add to favorites" });
    await expect(heart).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(heart);
    await expect(args.onToggleFavorite).toHaveBeenCalledTimes(1);
    await expect(canvas.getByRole("button", { name: "Remove from favorites" })).toHaveAttribute("aria-pressed", "true");
    await expect(args.onOpen).not.toHaveBeenCalled();
  },
};

export const BuyFires: Story = {
  args: { name: "Cyber Cap", price: "500", rarity: "epic", onBuy: fn(), onOpen: fn(), onToggleFavorite: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("button", { name: "Buy" }));
    await expect(args.onBuy).toHaveBeenCalledTimes(1);
    await expect(args.onOpen).not.toHaveBeenCalled();
  },
};

export const OpensOnBodyClick: Story = {
  args: { name: "Cyber Cap", price: "500", rarity: "epic", onOpen: fn(), onBuy: fn(), onToggleFavorite: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("button", { name: "Cyber Cap" }));
    await expect(args.onOpen).toHaveBeenCalledTimes(1);
  },
};

export const NotForSaleHidesBuy: Story = {
  args: { name: "Reserved Land", rarity: "mythic", onOpen: fn() },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText("Not for sale")).toBeInTheDocument();
    await expect(canvas.queryByRole("button", { name: "Buy" })).not.toBeInTheDocument();
  },
};
