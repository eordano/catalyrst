import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";
import NewShopFilterSidebar from "./NewShopFilterSidebar";
import { filterGroups } from "./fixtures";

const meta = {
  title: "Marketplace/NewShop/FilterSidebar",
  component: NewShopFilterSidebar,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
  args: {
    groups: filterGroups,
    itemCount: "5,373 items",
    onSale: true,
    onToggleOnSale: fn(),
    onOptionChange: fn(),
  },
} satisfies Meta<typeof NewShopFilterSidebar>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};
export const OnSaleOff: Story = { args: { onSale: false } };
