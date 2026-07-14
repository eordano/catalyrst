import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkCancelSalePage from "./MkCancelSalePage";

type Props = ComponentProps<typeof MkCancelSalePage>;
type CancelStatus = NonNullable<Props["status"]>;
type Ownership = NonNullable<Props["ownership"]>;

const SAMPLE_NFT: NonNullable<Props["nft"]> = {
  name: "Cyber Ronin Jacket",
  category: "wearable",
  rarity: "legendary",
  network: "ethereum",
};

/** `listed` is the order the page is normally shown with; `none` is "not for sale". */
const ORDERS = {
  listed: { price: "1,000", owner: "self" },
  none: null,
} satisfies Record<string, Props["order"]>;

type OrderKey = keyof typeof ORDERS;
const ORDER_KEYS = Object.keys(ORDERS) as OrderKey[];

const STATUSES = ["confirmation", "authorize", "pending", "success"] satisfies CancelStatus[];
const OWNERSHIPS = ["self", "other", "none"] satisfies Ownership[];

/** `orderKind` names a fixture; `status` and `ownership` stay real props. */
type CancelStoryArgs = { status: CancelStatus; ownership: Ownership; orderKind: OrderKey };

const meta = {
  title: "Marketplace/Pages/Cancel Sale",
  component: MkCancelSalePage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    status: { control: "select", options: STATUSES },
    ownership: { control: "select", options: OWNERSHIPS },
    orderKind: { control: "select", options: ORDER_KEYS },
  },
  args: { status: "confirmation", ownership: "self", orderKind: "listed" },
  render: ({ orderKind, ...rest }) => (
    <MkCancelSalePage {...rest} nft={SAMPLE_NFT} order={ORDERS[orderKind]} />
  ),
} satisfies Meta<CancelStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Every state at once. `Default` flips between them from the Controls panel; this story keeps
 * all six in the render + a11y + visual-diff gates, since `pending` and `success` return
 * entirely different status cards while `none`/`other` ownership swap the confirmation copy and
 * disable the CTA. `chrome={false}` because stacking N copies of `MarketplaceChrome` would emit
 * N `<main>` landmarks and fail axe's landmark-unique.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {(
        [
          { label: "confirmation", status: "confirmation", ownership: "self", orderKind: "listed" },
          { label: "authorize", status: "authorize", ownership: "self", orderKind: "listed" },
          { label: "pending", status: "pending", ownership: "self", orderKind: "listed" },
          { label: "success", status: "success", ownership: "self", orderKind: "listed" },
          { label: "not for sale", status: "confirmation", ownership: "none", orderKind: "none" },
          { label: "not the owner", status: "confirmation", ownership: "other", orderKind: "listed" },
        ] satisfies {
          label: string;
          status: CancelStatus;
          ownership: Ownership;
          orderKind: OrderKey;
        }[]
      ).map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          <MkCancelSalePage
            chrome={false}
            nft={SAMPLE_NFT}
            status={entry.status}
            ownership={entry.ownership}
            order={ORDERS[entry.orderKind]}
          />
        </section>
      ))}
    </div>
  ),
};
