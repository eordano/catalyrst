import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import NewShopRankTable from "./NewShopRankTable";

const FLOOR_ONLY = [
  { id: "a", name: "Crown Jewel", floor: "12,000", network: "polygon" as const },
  { id: "b", name: "Rare Relic", floor: "9,500", network: "polygon" as const },
  { id: "c", name: "Gilded Mask", floor: "8,100", network: "ethereum" as const },
];

const meta = {
  title: "Marketplace/NewShop/RankTable/Interactions",
  component: NewShopRankTable,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ maxWidth: 720, background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
} satisfies Meta<typeof NewShopRankTable>;
export default meta;

type Story = StoryObj<typeof meta>;

export const FloorOnlyHidesVolume: Story = {
  args: { title: "Most Valuable", rows: FLOOR_ONLY, onRow: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("columnheader", { name: "Floor price" })).toBeInTheDocument();
    await expect(canvas.queryByRole("columnheader", { name: "Volume" })).not.toBeInTheDocument();
    await userEvent.click(canvas.getByText("Crown Jewel"));
    await expect(args.onRow).toHaveBeenCalledWith("a");
  },
};

const WITH_VOLUME = FLOOR_ONLY.map((r, i) => ({ ...r, volume: String(1000 * (i + 1)) }));

export const WithVolumeShowsColumn: Story = {
  args: { title: "Top Assets", rows: WITH_VOLUME, onRow: fn() },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("columnheader", { name: "Volume" })).toBeInTheDocument();
  },
};
