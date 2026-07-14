import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import MkAssetPage from "./MkAssetPage";

const SAMPLE_NFT: NonNullable<ComponentProps<typeof MkAssetPage>["nft"]> = {
  name: "Cyber Ronin Jacket",
  issuedId: 142,
  category: "upper_body",
  rarity: "legendary",
  bodyShape: "Unisex",
  owner: { address: "0x9f3c2b71a4d5e6f8c0b1a2d3e4f5a6b7c8d9e0a1", name: "neon.dcl" },
  collection: { name: "Neon Districts", address: "0xc04528c14c8ffd84c7c1fb6719b4a89853035cdd" },
  order: { price: "4250", issuedId: 142, expiresLabel: "Expires in 27 days" },
};

const meta = {
  title: "Marketplace/Pages/AssetPage/Interactions",
  component: MkAssetPage,
  parameters: { layout: "fullscreen" },
  args: { nft: SAMPLE_NFT },
} satisfies Meta<typeof MkAssetPage>;
export default meta;

type Story = StoryObj<typeof meta>;

export const BuyOpensCheckout: Story = {
  args: { onBuy: fn(), onMakeOffer: fn(), onBack: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("button", { name: "Buy" }));
    await expect(args.onBuy).toHaveBeenCalledTimes(1);
  },
};

export const SecondaryCtasWired: Story = {
  args: { onBuy: fn(), onMakeOffer: fn(), onBack: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("button", { name: "Make an offer" }));
    await expect(args.onMakeOffer).toHaveBeenCalledTimes(1);
  },
};
