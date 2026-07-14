import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import MkBuyFlow from "./MkBuyFlow";

const SAMPLE_ASSET: NonNullable<ComponentProps<typeof MkBuyFlow>["asset"]> = {
  name: "Cyber Ronin Jacket",
  rarity: "legendary",
  network: "MATIC",
  kind: "wearable",
  priceMana: "1,250",
  priceUsd: "387.5000",
};

const meta = {
  title: "Marketplace/Workflows/MkBuyFlow/Interactions",
  component: MkBuyFlow,
  parameters: { layout: "fullscreen" },
  args: {
    asset: SAMPLE_ASSET,
    tokenBalance: "2,480.55",
    itemCostToken: "1,250",
    itemCostUsd: "387.5000",
    feeCostToken: "0.0241",
    feeCostUsd: "0.0182",
    totalToken: "1,250",
    totalUsd: "387.5182",
  },
} satisfies Meta<typeof MkBuyFlow>;
export default meta;

type Story = StoryObj<typeof meta>;

export const BuyNowConfirms: Story = {
  args: { onPrimary: fn(), onBack: fn(), onClose: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    const btn = canvas.getByRole("button", { name: "Buy now" });
    await expect(btn).toBeEnabled();
    await userEvent.click(btn);
    await expect(args.onPrimary).toHaveBeenCalledTimes(1);
  },
};

export const BuyingLocksUI: Story = {
  args: { state: "buying", onPrimary: fn(), onBack: fn(), onClose: fn() },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await expect(canvas.getByText(/Confirm Transaction in Your Wallet/)).toBeInTheDocument();
    await expect(canvas.queryByRole("button", { name: "Back" })).not.toBeInTheDocument();
    await expect(canvas.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
  },
};

export const LoadingRouteDisablesBuy: Story = {
  args: { state: "loadingRoute", onPrimary: fn() },
  play: async ({ canvasElement }) => {
    await within(canvasElement.ownerDocument.body).findByRole("dialog");
    const btn = canvasElement.ownerDocument.querySelector(".mkbuyflow__btn--primary") as HTMLButtonElement;
    await expect(btn).toBeDisabled();
  },
};

export const InsufficientOffersAlternatives: Story = {
  args: { state: "insufficient" },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await expect(canvas.getByText(/don.t have enough funds/)).toBeInTheDocument();
    await expect(canvas.getByRole("button", { name: "Get MANA" })).toBeInTheDocument();
    await expect(canvas.getByRole("button", { name: /Buy with card/ })).toBeInTheDocument();
    await expect(canvas.queryByRole("button", { name: "Buy now" })).not.toBeInTheDocument();
  },
};

export const RouteUnavailableOffersAlternatives: Story = {
  args: { state: "routeUnavailable", tokenSymbol: "USDC" },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await expect(canvas.getByText(/not available at the moment/)).toBeInTheDocument();
    await expect(canvas.getByRole("button", { name: "Get MANA" })).toBeInTheDocument();
    await expect(canvas.queryByRole("button", { name: "Buy now" })).not.toBeInTheDocument();
  },
};

export const PriceTooLowWarns: Story = {
  args: { state: "priceTooLow", onPrimary: fn() },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await expect(canvas.getByText(/only gas fee free if the item is at least 1 MANA/)).toBeInTheDocument();
    await expect(canvas.getByRole("button", { name: "Buy now" })).toBeInTheDocument();
  },
};

export const CardSubflowBackReturns: Story = {
  args: { state: "insufficient" },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await userEvent.click(canvas.getByRole("button", { name: /Buy with card/ }));
    await expect(canvas.getByRole("button", { name: "Continue" })).toBeInTheDocument();
    await userEvent.click(canvas.getByRole("button", { name: "Go back" }));
    await expect(canvas.getByRole("button", { name: "Get MANA" })).toBeInTheDocument();
  },
};

export const GetManaFromInsufficient: Story = {
  args: { state: "insufficient", onGetMana: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await userEvent.click(canvas.getByRole("button", { name: "Get MANA" }));
    await expect(args.onGetMana).toHaveBeenCalledTimes(1);
  },
};

export const CardContinueWired: Story = {
  args: { state: "insufficient", onContinueCard: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await userEvent.click(canvas.getByRole("button", { name: /Buy with card/ }));
    await userEvent.click(canvas.getByRole("button", { name: "Continue" }));
    await expect(args.onContinueCard).toHaveBeenCalledTimes(1);
  },
};

export const ChangeToken: Story = {
  args: { onTokenChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await userEvent.click(canvas.getByRole("button", { name: /Balance:/ }));
    const listbox = canvas.getByRole("listbox", { name: "Select token" });
    await userEvent.click(within(listbox).getByRole("option", { name: /USDC/ }));
    await expect(args.onTokenChange).toHaveBeenCalledWith("USDC");
    await expect(canvas.getByRole("button", { name: /USDC/ })).toBeInTheDocument();
  },
};

export const ChangeNetwork: Story = {
  args: { onChainChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await userEvent.click(canvas.getByRole("button", { name: /Polygon/ }));
    const listbox = canvas.getByRole("listbox", { name: "Select network" });
    await userEvent.click(within(listbox).getByRole("option", { name: /Ethereum/ }));
    await expect(args.onChainChange).toHaveBeenCalledWith("ethereum");
    await expect(canvas.getByRole("button", { name: /Ethereum/ })).toBeInTheDocument();
  },
};

export const SelectorsLockedWhileBuying: Story = {
  args: { state: "buying" },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await expect(canvas.getByRole("button", { name: /Polygon/ })).toBeDisabled();
  },
};

export const BackAndCloseWired: Story = {
  args: { onBack: fn(), onClose: fn(), onPrimary: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await canvas.findByRole("dialog");
    await userEvent.click(canvas.getByRole("button", { name: "Back" }));
    await expect(args.onBack).toHaveBeenCalledTimes(1);
    await userEvent.click(canvas.getByRole("button", { name: "Close" }));
    await expect(args.onClose).toHaveBeenCalledTimes(1);
  },
};
