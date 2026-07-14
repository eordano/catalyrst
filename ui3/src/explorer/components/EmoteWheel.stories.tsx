import type { Meta, StoryObj } from "@storybook/react-vite";
import EmoteWheel from "./EmoteWheel";
import type { Emote, SlotBinding } from "../../data/catalyst/backpack";

const RARITIES = [
  "base",
  "common",
  "uncommon",
  "rare",
  "epic",
  "legendary",
  "mythic",
  "exotic",
  "unique",
  "common",
];
const NAMES = [
  "Wave", "Clap", "Dance", "Kiss", "Head Explode",
  "Robot", "Hammer Time", "Tik Tok", "Snowfall", "Disco",
];

function makeEmote(i: number): Emote {
  return {
    urn: `urn:decentraland:off-chain:base-emotes:demo-${i}`,
    name: NAMES[i] ?? `Emote ${i}`,
    description: "",
    thumbnail: "",
    rarity: RARITIES[i] ?? "base",
    category: "miscellaneous",
    loop: false,
  };
}

const CATALOG: Emote[] = Array.from({ length: 10 }, (_, i) => makeEmote(i));
const LOADOUT: SlotBinding[] = CATALOG.map((e, i) => ({
  slot: (i + 1) % 10,
  urn: e.urn,
  name: e.name,
}));

const meta = {
  title: "Explorer/Components/EmoteWheel",
  component: EmoteWheel,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof EmoteWheel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { catalog: CATALOG, loadout: LOADOUT },
};

export const EmptySlots: Story = {
  args: { catalog: [], loadout: [] },
};

export const PartiallyBound: Story = {
  args: { catalog: CATALOG, loadout: LOADOUT.slice(0, 4) },
};
