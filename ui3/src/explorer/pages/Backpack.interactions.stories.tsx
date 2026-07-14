import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import Backpack from "./Backpack";

const HAT = {
  urn: "urn:test:hat:1",
  name: "Cool Hat",
  category: "hat",
  rarity: "rare",
  thumbnail: "",
};

const meta = {
  title: "Explorer/Pages/Backpack/Interactions",
  component: Backpack,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Backpack>;
export default meta;

type Story = StoryObj<typeof meta>;

export const EquipWearable: Story = {
  args: {
    catalog: [HAT],
    equipped: {
      wearables: [],
      bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale",
      skinColor: "#c98c63",
      hairColor: "#5c3824",
      eyeColor: "#3a6ea5",
      emotes: [],
    },
    onEquippedChange: fn(),
  },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByTitle("Cool Hat"));
    await expect(args.onEquippedChange).toHaveBeenCalledWith(["urn:test:hat:1"]);
  },
};

const GLASSES = {
  urn: "urn:test:glasses:1",
  name: "Cool Glasses",
  category: "eyewear",
  rarity: "rare",
  thumbnail: "",
};

const BASE_EQUIPPED = {
  wearables: [],
  bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale",
  skinColor: "#c98c63",
  hairColor: "#5c3824",
  eyeColor: "#3a6ea5",
  emotes: [],
};

export const FilterByCategory: Story = {
  args: { catalog: [HAT, GLASSES], equipped: BASE_EQUIPPED },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByTitle("Cool Hat")).toBeInTheDocument();
    // Only categories present in the catalog get a rail tile.
    await expect(canvas.queryByRole("button", { name: "Mouth" })).not.toBeInTheDocument();
    await userEvent.click(canvas.getByRole("button", { name: "Eyewear" }));
    await expect(canvas.queryByTitle("Cool Hat")).not.toBeInTheDocument();
    await expect(canvas.getByTitle("Cool Glasses")).toBeInTheDocument();
    await userEvent.click(canvas.getByRole("button", { name: "Hat" }));
    await expect(canvas.getByTitle("Cool Hat")).toBeInTheDocument();
  },
};
