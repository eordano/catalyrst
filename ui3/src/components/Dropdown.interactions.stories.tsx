import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import Dropdown from "./Dropdown";

const OPTIONS = ["Newest", "Oldest", "Popular"];

const meta = {
  title: "Components/Dropdown/Interactions",
  component: Dropdown,
  tags: ["autodocs"],
} satisfies Meta<typeof Dropdown>;
export default meta;

type Story = StoryObj<typeof meta>;

export const OpenAndSelect: Story = {
  args: { ariaLabel: "Sort", options: OPTIONS, onChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const btn = canvas.getByRole("button", { name: "Sort" });
    await expect(btn).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(btn);
    await expect(btn).toHaveAttribute("aria-expanded", "true");
    await userEvent.click(canvas.getByRole("option", { name: "Popular" }));
    await expect(args.onChange).toHaveBeenCalledWith("Popular");
    await expect(btn).toHaveAttribute("aria-expanded", "false");
    await expect(btn).toHaveTextContent("Popular");
  },
};

export const KeyboardSelect: Story = {
  args: { ariaLabel: "Sort", options: OPTIONS, onChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const btn = canvas.getByRole("button", { name: "Sort" });
    btn.focus();
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{Enter}");
    await expect(args.onChange).toHaveBeenCalledWith("Oldest");
  },
};
