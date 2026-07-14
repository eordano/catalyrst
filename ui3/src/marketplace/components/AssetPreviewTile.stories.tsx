import type { Meta, StoryObj } from "@storybook/react-vite";
import AssetPreviewTile from "./AssetPreviewTile";
import ManaMark from "../../atoms/ManaMark";

const THUMB = (urn: string) =>
  `https://peer.decentraland.org/lambdas/collections/contents/${urn}/thumbnail`;

const ITEMS = {
  common: { name: "Macarena", urn: "urn:decentraland:matic:collections-v2:0xb53081d7db78456eb436702c2f50a90436c8633f:0" },
  uncommon: { name: "Pride Jetpack", urn: "urn:decentraland:matic:collections-v2:0xee6d52f3f05eb4540ab754d0f3fabbf97f8555fc:0" },
  rare: { name: "Wren", urn: "urn:decentraland:matic:collections-v2:0x2e3a685b2cbcf10ded65860f8bc63a865658ae0c:0" },
  epic: { name: "Stand with Crypto Hoodie", urn: "urn:decentraland:matic:collections-v2:0x2e4615470f56bbb5780262b9297bc4e968e8fb3e:0" },
  legendary: { name: "Skillit Hoodie", urn: "urn:decentraland:matic:collections-v2:0xe2750c571568e434d5351d25a97bcb0ea12ceed0:4" },
  mythic: { name: "Potion Maker Top", urn: "urn:decentraland:matic:collections-v2:0x11b804bec37cddc70aef6eab0b88d6c6c59c4621:0" },
  unique: { name: "Relic of Opulence", urn: "urn:decentraland:matic:collections-v2:0x2d19c4f2212ec37ed48eb8134ffbc0a695612986:0" },
  exotic: { name: "Space Force Ones", urn: "urn:decentraland:matic:collections-v2:0xa780096910ce7fc4663ad3a954b34e2d842f0374:0" },
};
type ItemKey = keyof typeof ITEMS;
const RARITIES = Object.keys(ITEMS) as ItemKey[];
const img = (r: ItemKey) => THUMB(ITEMS[r].urn);
const alt = (r: ItemKey) => ITEMS[r].name;

/** `image` is picked by rarity name; `none` is a first-class option. */
const IMAGE = {
  none: null,
  ...Object.fromEntries(RARITIES.map((r) => [r, img(r)])),
} as Record<"none" | ItemKey, string | null>;

/** `label === undefined` falls back to the rarity chip; `null` hides the chip entirely. */
const LABEL = {
  rarity: undefined,
  none: null,
  forSale: "For sale",
};

const CHILDREN = {
  none: undefined,
  mana: <ManaMark size={22} />,
};

const meta = {
  title: "Marketplace/Components/AssetPreviewTile",
  component: AssetPreviewTile,
  parameters: {
    layout: "centered",
    backgrounds: { default: "panel", values: [{ name: "panel", value: "#161518" }] },
  },
  decorators: [
    (Story) => (
      <div style={{ minWidth: 280, boxSizing: "border-box" }}>
        <Story />
      </div>
    ),
  ],
  argTypes: {
    rarity: { control: "select", options: RARITIES },
    figure: { control: "select", options: ["glyph", "plate", "inset", "none"] },
    chipPosition: { control: "inline-radio", options: ["top", "bottom"] },
    label: { control: "select", options: Object.keys(LABEL), mapping: LABEL },
    image: { control: "select", options: Object.keys(IMAGE), mapping: IMAGE },
    alt: { control: "text" },
    size: { control: "number" },
    radius: { control: "number" },
    children: { control: "select", options: Object.keys(CHILDREN), mapping: CHILDREN },
  },
  args: {
    rarity: "legendary",
    figure: "glyph",
    chipPosition: "top",
    label: "rarity",
    image: "legendary",
    alt: alt("legendary"),
    size: 240,
    radius: 16,
    children: "none",
  },
} satisfies Meta<typeof AssetPreviewTile>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const cell = { font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" };

/**
 * Every rarity plus each figure / chip / label variant. `Default` flips between them from the
 * Controls panel; this keeps all of them in the render + a11y + visual-diff gates.
 */
export const Catalog: Story = {
  name: "Catalog (every rarity + figure)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
      <section>
        <div style={cell}>Every rarity</div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 16 }}>
          {RARITIES.map((r) => (
            <AssetPreviewTile
              key={r}
              rarity={r}
              image={img(r)}
              alt={alt(r)}
              style={{ width: 150, height: 150 }}
            />
          ))}
        </div>
      </section>
      <section>
        <div style={cell}>Figures, chips and labels</div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 16, alignItems: "flex-start" }}>
          <AssetPreviewTile rarity="mythic" figure="plate" chipPosition="bottom" size={150} />
          <AssetPreviewTile rarity="epic" figure="inset" label={null} size={150} />
          <AssetPreviewTile rarity="rare" label="For sale" image={img("rare")} alt={alt("rare")} size={150} />
          <AssetPreviewTile rarity="common" figure="none" size={150} />
          <AssetPreviewTile rarity="legendary" label={null} size={55} radius={6}>
            <ManaMark size={22} />
          </AssetPreviewTile>
        </div>
      </section>
    </div>
  ),
};
