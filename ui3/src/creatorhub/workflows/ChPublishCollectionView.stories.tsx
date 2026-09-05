import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";

import ChPublishCollectionView from "./ChPublishCollectionView";
import type { ChCollection, ChCollectionItem } from "../pages/ChCollectionDetail";

type Props = ComponentProps<typeof ChPublishCollectionView>;
type Fee = NonNullable<Props["fee"]>;

const collection: ChCollection = {
  name: "Genesis Capsule Vol.1",
  status: "unsynced",
  isPublished: false,
  isApproved: false,
  isOnSale: false,
  isLocked: false,
};

const wearables: ChCollectionItem[] = [
  { id: "w1", name: "Cyber Visor", rarity: "epic", category: "eyewear", price: "75", supply: null, status: "ready", smart: false, hue: 212 },
  { id: "w2", name: "Neon Hoodie", rarity: "rare", category: "upper_body", price: "40", supply: null, status: "ready", smart: false, hue: 282 },
  { id: "w3", name: "Holo Sneakers", rarity: "legendary", category: "feet", price: null, supply: null, status: "not_ready", smart: false, hue: 24 },
  { id: "w4", name: "Pixel Crown", rarity: "mythic", category: "hat", price: "500", supply: null, status: "ready", smart: false, hue: 332 },
];

const emotes: ChCollectionItem[] = [
  { id: "e1", name: "Victory Dance", rarity: "rare", category: "dance", playMode: "loop", price: "30", supply: null, status: "ready", hue: 196 },
  { id: "e2", name: "Slow Clap", rarity: "common", category: "reaction_positive", playMode: "simple", price: "10", supply: null, status: "ready", hue: 48 },
];

const summary = { collection, wearables, emotes };

const fee: Fee = {
  lines: [
    { rarity: "mythic", count: 1, manaPerItem: 100, mana: 100 },
    { rarity: "legendary", count: 1, manaPerItem: 100, mana: 100 },
    { rarity: "epic", count: 1, manaPerItem: 100, mana: 100 },
    { rarity: "rare", count: 2, manaPerItem: 100, mana: 200 },
    { rarity: "common", count: 1, manaPerItem: 100, mana: 100 },
  ],
  itemCount: 6,
  manaPerItem: 100,
  totalMana: 600,
};

const noop = () => {};

const VIEWS = ["summary", "cost", "terms", "pay", "submitted", "error", "blocked"];

const meta = {
  title: "CreatorHub/Workflows/Publish Collection",
  component: ChPublishCollectionView,
  parameters: { layout: "fullscreen" },
  argTypes: {
    view: { control: "select", options: VIEWS, description: "Which wizard screen renders." },
    step: { control: "select", options: VIEWS, description: "Mirrored onto `data-step`." },
    accepted: { control: "boolean", description: "Terms screen: checkbox ticked, Accept enabled." },
    collectionName: { control: "text" },
    summary: { control: "object", description: "`null` is what drives the blocked/empty screen." },
    txHash: { control: "text" },
    statusHref: { control: "text" },
    error: { control: "text" },
  },
  args: {
    step: "summary",
    view: "summary",
    collectionName: collection.name,
    summary,
    fee,
    accepted: false,
    txHash: "0x9f3c4d1e7a2188cf90b3a6e7c4d5f6a7b8c9d0e1",
    error: "MetaMask: user rejected the transaction.",
    onNext: noop,
    onBack: noop,
    onAccept: noop,
    onAcceptedChange: noop,
    onRetry: noop,
    onDone: noop,
  },
} satisfies Meta<typeof ChPublishCollectionView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Kept as its own export. The panel ids are `useId()`-derived now, so that is no longer the
 * reason -- the Terms heading is the fixed string "Content & curation terms", so two Terms
 * regions on one page carry the same accessible name whatever their ids are, and axe's
 * landmark-unique fires (observed: `landmark-unique` on the second `section[aria-labelledby]`).
 * The catalog can therefore hold only one Terms entry; `accepted: true` lives here.
 */
export const TermsAccepted: Story = {
  args: { step: "terms", view: "terms", accepted: true },
};

const CASES: { label: string; args: Partial<Props> }[] = [
  { label: "1 \u{B7} Review collection", args: { step: "summary", view: "summary" } },
  { label: "2 \u{B7} Publish fee", args: { step: "cost", view: "cost" } },
  { label: "3 \u{B7} Terms", args: { step: "terms", view: "terms" } },
  { label: "4 \u{B7} Paying", args: { step: "pay", view: "pay" } },
  { label: "5 \u{B7} Submitted", args: { step: "submitted", view: "submitted" } },
  { label: "Error", args: { step: "error", view: "error" } },
  {
    label: "Blocked \u{B7} empty draft",
    args: {
      step: "blocked",
      view: "blocked",
      summary: null,
      collectionName: "Empty Draft Collection",
    },
  },
];

export const Catalog: Story = {
  name: "Catalog (every step)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 40, padding: 24 }}>
      {CASES.map((c) => (
        <section key={c.label} aria-label={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <ChPublishCollectionView {...args} {...c.args} />
        </section>
      ))}
    </div>
  ),
};
