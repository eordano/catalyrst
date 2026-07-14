import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import Tabs from "./Tabs";

const TABS = [
  { id: "all", label: "All" },
  { id: "mine", label: "Mine" },
  { id: "saved", label: "Saved" },
];

const meta = {
  title: "Explorer/Components/Tabs/Interactions",
  component: Tabs,
} satisfies Meta<typeof Tabs>;
export default meta;

type Story = StoryObj<typeof meta>;

export const ClickSelectsTab: Story = {
  args: { tabs: TABS, onChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("tab", { name: "All" })).toHaveAttribute("aria-selected", "true");
    await userEvent.click(canvas.getByRole("tab", { name: "Mine" }));
    await expect(args.onChange).toHaveBeenCalledWith("mine");
    await expect(canvas.getByRole("tab", { name: "Mine" })).toHaveAttribute("aria-selected", "true");
  },
};

export const ArrowKeyMovesSelection: Story = {
  args: { tabs: TABS, onChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    canvas.getByRole("tab", { name: "All" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    await expect(args.onChange).toHaveBeenCalledWith("mine");
  },
};
