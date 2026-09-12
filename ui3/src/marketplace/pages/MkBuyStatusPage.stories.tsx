import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkBuyStatusPage from "./MkBuyStatusPage";

type BuyStatus = NonNullable<ComponentProps<typeof MkBuyStatusPage>["status"]>;

const SAMPLE_ASSET = {
  name: "Cyber Ronin Jacket",
  rarity: "legendary",
  category: "wearable",
};

const STATUSES = [
  "pending",
  "complete",
  "failed",
  "cancelled",
  "refunded",
] satisfies BuyStatus[];

const meta = {
  title: "Marketplace/Pages/Buy Status",
  component: MkBuyStatusPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    status: { control: "select", options: STATUSES },
  },
  args: { asset: SAMPLE_ASSET, status: "pending" },
} satisfies Meta<typeof MkBuyStatusPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Pending: Story = {
  args: { status: "pending" },
};

export const Refunded: Story = {
  args: { status: "refunded" },
};

export const Catalog: Story = {
  name: "Catalog (every status)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {STATUSES.map((status) => (
        <section key={status}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {status}
          </div>
          <MkBuyStatusPage chrome={false} asset={SAMPLE_ASSET} status={status} />
        </section>
      ))}
    </div>
  ),
};
