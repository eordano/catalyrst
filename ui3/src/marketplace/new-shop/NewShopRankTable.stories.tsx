import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";
import NewShopRankTable from "./NewShopRankTable";
import { rankRows } from "./fixtures";

const meta = {
  title: "Marketplace/NewShop/RankTable",
  component: NewShopRankTable,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ maxWidth: 720, background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
  args: { title: "Top Assets", rows: rankRows, onRow: fn() },
} satisfies Meta<typeof NewShopRankTable>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};
