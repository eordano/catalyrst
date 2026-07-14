import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkCollectionPage from "./MkCollectionPage";

type Props = ComponentProps<typeof MkCollectionPage>;

const SAMPLE_COLLECTION: NonNullable<Props["collection"]> = {
  name: "Neon Runners Wardrobe",
  isOnSale: true,
};

const SAMPLE_ITEMS: NonNullable<Props["items"]> = [
  { id: "i1", name: "Neon Pulse Visor", category: "wearable", sub: "hat", rarity: "legendary", available: 64, price: "180" },
  { id: "i2", name: "Circuit Bomber Jacket", category: "wearable", sub: "upper_body", rarity: "epic", available: 412, price: "95" },
  { id: "i3", name: "Glow Cargo Pants", category: "wearable", sub: "lower_body", rarity: "rare", available: 1820, price: "40" },
  { id: "i4", name: "Holo Runner Boots", category: "wearable", sub: "feet", rarity: "uncommon", available: 7340, price: "18" },
  { id: "i5", name: "Spectral Shades", category: "wearable", sub: "eyewear", rarity: "mythic", available: 3, price: "1,250" },
  { id: "i6", name: "Datastream Mohawk", category: "wearable", sub: "hair", rarity: "common", available: 41200, price: "6" },
  { id: "i7", name: "Voltage Helmet", category: "wearable", sub: "helmet", rarity: "unique", available: 1, price: "\u{2014}" },
  { id: "e1", name: "Power Surge", category: "emote", sub: "dance", rarity: "epic", available: 380, price: "75" },
  { id: "e2", name: "Glitch Wave", category: "emote", sub: "fun", rarity: "rare", available: 2640, price: "32" },
  { id: "e3", name: "Neon Bow", category: "emote", sub: "greetings", rarity: "legendary", available: 88, price: "210" },
];

/** The exact four the old `WearablesOnly` story listed -- enough to drop the emote tab strip. */
const WEARABLES_ONLY: NonNullable<Props["items"]> = SAMPLE_ITEMS.filter((i) =>
  ["i1", "i2", "i3", "i7"].includes(i.id)
);

/** The two item sets the page has been shown with, picked by name. */
const ITEMS = { mixed: SAMPLE_ITEMS, wearablesOnly: WEARABLES_ONLY } satisfies Record<
  string,
  NonNullable<Props["items"]>
>;

type ItemsKey = keyof typeof ITEMS;
const ITEMS_KEYS = Object.keys(ITEMS) as ItemsKey[];

const STATES = ["ready", "loading", "empty"] satisfies NonNullable<Props["state"]>[];

/** `itemSet` names a fixture; `state` and `isOwner` stay real props. */
type CollectionStoryArgs = {
  itemSet: ItemsKey;
  state: NonNullable<Props["state"]>;
  isOwner: boolean;
};

const meta = {
  title: "Marketplace/Pages/Collection",
  component: MkCollectionPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    itemSet: { control: "select", options: ITEMS_KEYS },
    state: { control: "select", options: STATES },
    isOwner: { control: "boolean" },
  },
  args: { itemSet: "mixed", state: "ready", isOwner: false },
  render: ({ itemSet, ...rest }) => (
    <MkCollectionPage {...rest} collection={SAMPLE_COLLECTION} items={ITEMS[itemSet]} />
  ),
} satisfies Meta<CollectionStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Every state at once. `Default` flips between them from the Controls panel; this story keeps
 * all five in the render + a11y + visual-diff gates: `isOwner` adds the owner action bar, a
 * wearables-only item set drops the wearables/emotes tab strip entirely, and `loading` /
 * `empty` replace the table with a spinner and an empty-state card. `chrome={false}` because
 * stacking N copies of `MarketplaceChrome` would emit N `<main>` landmarks and fail axe's
 * landmark-unique.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {(
        [
          { label: "ready", itemSet: "mixed", state: "ready", isOwner: false },
          { label: "collection owner", itemSet: "mixed", state: "ready", isOwner: true },
          { label: "wearables only (no tabs)", itemSet: "wearablesOnly", state: "ready", isOwner: false },
          { label: "empty", itemSet: "mixed", state: "empty", isOwner: false },
          { label: "loading", itemSet: "mixed", state: "loading", isOwner: false },
        ] satisfies {
          label: string;
          itemSet: ItemsKey;
          state: NonNullable<Props["state"]>;
          isOwner: boolean;
        }[]
      ).map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          <MkCollectionPage
            chrome={false}
            collection={SAMPLE_COLLECTION}
            items={ITEMS[entry.itemSet]}
            state={entry.state}
            isOwner={entry.isOwner}
          />
        </section>
      ))}
    </div>
  ),
};
