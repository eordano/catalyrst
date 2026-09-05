import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import SearchField from "./SearchField";

const meta = {
  title: "Atoms/SearchField/Interactions",
  component: SearchField,
  tags: ["autodocs"],
} satisfies Meta<typeof SearchField>;
export default meta;

type Story = StoryObj<typeof meta>;

export const TypingFiresOnChange: Story = {
  args: { placeholder: "Search", onChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const input = canvas.getByRole("textbox", { name: "Search" });
    await userEvent.type(input, "hello");
    await expect(input).toHaveValue("hello");
    await expect(args.onChange).toHaveBeenCalledTimes(5);
    await expect(args.onChange).toHaveBeenLastCalledWith("hello");
  },
};
