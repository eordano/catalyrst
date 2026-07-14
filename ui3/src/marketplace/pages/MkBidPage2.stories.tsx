import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkBidPage2 from "./MkBidPage2";

const SAMPLE_ITEM: NonNullable<ComponentProps<typeof MkBidPage2>["item"]> = {
  name: "Pioneer Jacket",
  collection: "Decentraland Wearables",
  rarity: "legendary",
  network: "MATIC",
};

const meta = {
  title: "Marketplace/Pages/Bid (Item)",
  component: MkBidPage2,
  parameters: { layout: "fullscreen" },
  argTypes: {
    manaBalance: { control: "text" },
    submitting: { control: "boolean" },
    insufficientMana: { control: "boolean" },
    lowPriceWarn: { control: "boolean" },
    confirming: { control: "boolean" },
  },
  args: {
    item: SAMPLE_ITEM,
    manaBalance: "2,480.55",
    submitting: false,
    insufficientMana: false,
    lowPriceWarn: false,
    confirming: false,
  },
} satisfies Meta<typeof MkBidPage2>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

type CatalogEntry = {
  label: string;
  manaBalance: string;
  submitting: boolean;
  insufficientMana: boolean;
  lowPriceWarn: boolean;
  confirming?: boolean;
};

const CATALOG: CatalogEntry[] = [
  { label: "form", manaBalance: "2,480.55", submitting: false, insufficientMana: false, lowPriceWarn: false },
  { label: "submitting", manaBalance: "2,480.55", submitting: true, insufficientMana: false, lowPriceWarn: false },
  { label: "insufficient MANA", manaBalance: "120.00", submitting: false, insufficientMana: true, lowPriceWarn: false },
  { label: "low price warning", manaBalance: "2,480.55", submitting: false, insufficientMana: false, lowPriceWarn: true },
  { label: "confirming", manaBalance: "2,480.55", submitting: false, insufficientMana: false, lowPriceWarn: false, confirming: true },
];

/**
 * Every state at once, the confirm dialog included. `Default` flips between them from the
 * Controls panel; this story keeps all five in the render + a11y + visual-diff gates, since each
 * flag adds or replaces a different block (spinner, insufficient-MANA notice, low-price warning,
 * confirm dialog). `chrome={false}` because stacking N copies of `MarketplaceChrome` would emit
 * N `<main>` landmarks and fail axe's landmark-unique. `portal={false}` on the `confirming` entry
 * lays the same `Modal` card out in normal document flow instead of `createPortal`ing a
 * `position: fixed; inset: 0` backdrop onto `document.body`, which would scrim every other entry
 * in the shared screenshot.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CATALOG.map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          <MkBidPage2
            chrome={false}
            portal={false}
            item={SAMPLE_ITEM}
            manaBalance={entry.manaBalance}
            submitting={entry.submitting}
            insufficientMana={entry.insufficientMana}
            lowPriceWarn={entry.lowPriceWarn}
            confirming={entry.confirming}
          />
        </section>
      ))}
    </div>
  ),
};
