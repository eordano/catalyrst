import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import Toggle from "./Toggle";

const meta = {
  title: "Atoms/Toggle/Interactions",
  component: Toggle,
  tags: ["autodocs"],
} satisfies Meta<typeof Toggle>;
export default meta;

type Story = StoryObj<typeof meta>;

export const TogglesOnClick: Story = {
  args: { ariaLabel: "Notifications", onChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const sw = canvas.getByRole("switch", { name: "Notifications" });
    await expect(sw).toHaveAttribute("aria-checked", "false");
    await userEvent.click(sw);
    await expect(sw).toHaveAttribute("aria-checked", "true");
    await expect(args.onChange).toHaveBeenCalledWith(true);
  },
};
