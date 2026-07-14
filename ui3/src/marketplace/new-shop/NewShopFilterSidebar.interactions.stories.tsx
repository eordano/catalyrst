import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, fn, userEvent, within } from "storybook/test";
import NewShopFilterSidebar from "./NewShopFilterSidebar";
import { filterGroups } from "./fixtures";

const meta = {
  title: "Marketplace/NewShop/FilterSidebar/Interactions",
  component: NewShopFilterSidebar,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
  args: { groups: filterGroups, onSale: true, onToggleOnSale: fn(), onOptionChange: fn() },
} satisfies Meta<typeof NewShopFilterSidebar>;
export default meta;

type Story = StoryObj<typeof meta>;

export const LabelledGroups: Story = {
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("group", { name: "Category" })).toBeInTheDocument();
    await expect(canvas.getByRole("group", { name: "Rarity" })).toBeInTheDocument();

    const sw = canvas.getByRole("switch", { name: "On Sale" });
    await expect(sw).toHaveAttribute("aria-checked", "true");
    await userEvent.click(sw);
    await expect(args.onToggleOnSale).toHaveBeenCalledWith(false);

    await fireEvent.click(canvas.getByRole("checkbox", { name: "Emotes" }));
    await expect(args.onOptionChange).toHaveBeenCalledWith("category", "emotes", true);
  },
};
