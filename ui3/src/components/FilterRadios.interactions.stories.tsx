import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, fn, within } from "storybook/test";
import FilterRadios from "./FilterRadios";

const OPTIONS = [
  { id: "all", label: "All" },
  { id: "onsale", label: "On sale" },
  { id: "free", label: "Free" },
];

const meta = {
  title: "Components/FilterRadios/Interactions",
  component: FilterRadios,
  tags: ["autodocs"],
} satisfies Meta<typeof FilterRadios>;
export default meta;

type Story = StoryObj<typeof meta>;

export const SelectFiresOnChange: Story = {
  args: { name: "status", value: "all", options: OPTIONS, onChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await fireEvent.click(canvas.getByRole("radio", { name: "On sale" }));
    await expect(args.onChange).toHaveBeenCalledWith("onsale");
  },
};
