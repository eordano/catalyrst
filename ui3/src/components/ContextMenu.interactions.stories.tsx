import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import ContextMenu from "./ContextMenu";

const meta = {
  title: "Components/ContextMenu/Interactions",
  component: ContextMenu,
  tags: ["autodocs"],
} satisfies Meta<typeof ContextMenu>;
export default meta;

type Story = StoryObj<typeof meta>;

const onRename = fn();

export const ItemClickFires: Story = {
  args: {
    items: [
      { kind: "button", label: "Rename", onClick: onRename },
      { kind: "separator" },
      { kind: "button", label: "Delete", danger: true },
    ],
  },
  play: async ({ canvasElement }) => {
    onRename.mockClear();
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("menuitem", { name: "Rename" }));
    await expect(onRename).toHaveBeenCalledTimes(1);
  },
};

export const EscapeCloses: Story = {
  args: {
    autoFocus: true,
    onClose: fn(),
    items: [{ kind: "button", label: "Rename" }],
  },
  play: async ({ args }) => {
    await userEvent.keyboard("{Escape}");
    await expect(args.onClose).toHaveBeenCalledTimes(1);
  },
};
