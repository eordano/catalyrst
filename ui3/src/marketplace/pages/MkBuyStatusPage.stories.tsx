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

/**
 * Kept as its own export rather than folded into `Default`: the story id
 * `marketplace-pages-buy-status--pending` is a screen-tour deep link
 * (`tools/screen-tour/add-story-links.mts`).
 */
export const Pending: Story = {
  args: { status: "pending" },
};

/** Kept for the same reason -- `marketplace-pages-buy-status--refunded` is a screen-tour deep link. */
export const Refunded: Story = {
  args: { status: "refunded" },
};

/**
 * Every `status` at once. `Default` flips between them from the Controls panel; this story
 * keeps all five in the render + a11y + visual-diff gates, since each status renders a
 * different title/status-line/description/CTA subtree. `chrome={false}` because stacking N
 * copies of `MarketplaceChrome` would emit N `<main>` landmarks and fail axe's landmark-unique.
 */
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
