import type { Meta, StoryObj } from "@storybook/react-vite";
import StProfileMyAssetsTab from "./StProfileMyAssetsTab";
import type { AssetItem, NameItem, Profile } from "./StProfileMyAssetsTab";

const PROFILE: Profile = {
  address: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
  name: "PixelNomad",
  hasClaimedName: true,
  nameColor: "#FF8362",
};

const WEARABLES: AssetItem[] = [
  { id: "w1", name: "Cyber Halo", rarity: "epic", price: "350", network: "MATIC", category: "hat", bodyShape: "unisex", isSmart: false },
  { id: "w2", name: "Aurora Jacket", rarity: "legendary", price: "1,200", network: "MATIC", category: "upper_body", bodyShape: "female", isSmart: false },
  { id: "w3", name: "Glitch Sneakers", rarity: "rare", price: null, network: "MATIC", category: "feet", bodyShape: "male", isSmart: false },
  { id: "w4", name: "Founders Crown", rarity: "mythic", price: null, network: "ETHEREUM", category: "tiara", bodyShape: "unisex", isSmart: false },
  { id: "w5", name: "Neon Visor", rarity: "uncommon", price: "42", network: "MATIC", category: "eyewear", bodyShape: "unisex", isSmart: true },
  { id: "w6", name: "Plasma Gauntlets", rarity: "exotic", price: "5,000", network: "MATIC", category: "hands_wear", bodyShape: "male", isSmart: false },
  { id: "w7", name: "Void Cloak", rarity: "unique", price: null, network: "ETHEREUM", category: "upper_body", bodyShape: "unisex", isSmart: false },
  { id: "w8", name: "Static Mask", rarity: "common", price: "18", network: "MATIC", category: "mask", bodyShape: "unisex", isSmart: false },
];

const NAMES: NameItem[] = [
  { id: "n1", stem: "pixelnomad" },
  { id: "n2", stem: "neondreams" },
  { id: "n3", stem: "plaza42" },
];

/** The inventory is picked by name: the wearable + name fixtures, or nothing owned. */
const INVENTORY = {
  owned: { wearables: WEARABLES, names: NAMES },
  none: { wearables: [] as AssetItem[], names: [] as NameItem[] },
};
type InventoryKey = keyof typeof INVENTORY;

/** The category chips the component itself enumerates. */
const CATEGORIES = ["wearable", "emote", "ens", "parcel", "estate"] as const;
type CategoryKey = (typeof CATEGORIES)[number];

/** Story args: the inventory is picked by name, the rest are real props. */
type MyAssetsStoryArgs = {
  inventory: InventoryKey;
  category: CategoryKey;
  loading: boolean;
  empty: boolean;
};

const BASE: MyAssetsStoryArgs = {
  inventory: "owned",
  category: "wearable",
  loading: false,
  empty: false,
};

function renderTab({
  inventory,
  category,
  loading,
  empty,
  chrome,
  labelSuffix,
}: MyAssetsStoryArgs & { chrome?: boolean; labelSuffix?: string }) {
  return (
    <StProfileMyAssetsTab
      profile={PROFILE}
      wearables={INVENTORY[inventory].wearables}
      names={INVENTORY[inventory].names}
      category={category}
      loading={loading}
      empty={empty}
      chrome={chrome}
      labelSuffix={labelSuffix}
    />
  );
}

const meta = {
  title: "Web/Pages/Profile/My Assets",
  component: StProfileMyAssetsTab,
  parameters: { layout: "fullscreen" },
  argTypes: {
    inventory: {
      control: "select",
      options: ["owned", "none"],
      description: "Which fixture is passed \u{2014} the eight wearables + three names, or nothing owned.",
    },
    category: {
      control: "select",
      options: CATEGORIES,
      description: "Which category chip is active; `ens` swaps the wearable grid for the name grid.",
    },
    loading: { control: "boolean", description: "Renders the skeleton grid." },
    empty: { control: "boolean", description: "Forces the empty state regardless of the fixture." },
  },
  args: BASE,
  render: renderTab,
} satisfies Meta<MyAssetsStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; args: MyAssetsStoryArgs }[] = [
  { label: "wearables", args: BASE },
  { label: "names", args: { ...BASE, category: "ens" } },
  { label: "empty", args: { inventory: "none", category: "wearable", loading: false, empty: true } },
  { label: "loading", args: { inventory: "none", category: "wearable", loading: true, empty: false } },
];

/**
 * Every state at once. `Default` flips between them from the Controls panel; this keeps the
 * wearable grid, the NAMEs grid, the empty state and the skeleton in the render + a11y +
 * visual-diff gates. `chrome={false}` so stacking does not emit N `<main>` landmarks, and
 * `labelSuffix` makes each copy's `nav[aria-label="Profile sections"]` uniquely named --
 * axe's `landmark-unique` compares accessible names, so a fixed label would fail N times.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32 }}>
      {CATALOG.map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          {renderTab({ ...entry.args, chrome: false, labelSuffix: `(${entry.label})` })}
        </section>
      ))}
    </div>
  ),
};
