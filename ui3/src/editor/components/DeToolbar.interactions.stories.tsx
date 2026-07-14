import type { ComponentType } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import { DeToolbar as DeToolbarImpl } from "../pages/DeWorkspace";

const DeToolbar = DeToolbarImpl as unknown as ComponentType<Record<string, unknown>>;

const meta = {
  title: "Editor/Components/Toolbar/Interactions",
  component: DeToolbar,
} satisfies Meta<typeof DeToolbar>;
export default meta;

type Story = StoryObj<typeof meta>;

export const SelectToolFiresOnTool: Story = {
  args: { tool: "translate", onTool: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByTitle("Move (W)")).toHaveClass("active");
    await userEvent.click(canvas.getByTitle("Rotate (E)"));
    await expect(args.onTool).toHaveBeenCalledWith("rotate");
  },
};

export const PlayAndStep: Story = {
  args: { playing: false, onPlay: fn(), onStep: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByTitle("Run the scene"));
    await expect(args.onPlay).toHaveBeenCalledTimes(1);
    await userEvent.click(canvas.getByTitle("Advance one tick"));
    await expect(args.onStep).toHaveBeenCalledTimes(1);
  },
};

export const PlayingShowsPause: Story = {
  args: { playing: true, onPause: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.queryByTitle("Run the scene")).not.toBeInTheDocument();
    await userEvent.click(canvas.getByTitle("Scene is running — pause"));
    await expect(args.onPause).toHaveBeenCalledTimes(1);
  },
};

export const StopRestarts: Story = {
  args: { playing: false, onStop: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByTitle("Restart the scene from tick 0"));
    await expect(args.onStop).toHaveBeenCalledTimes(1);
  },
};

export const UnavailableActionsDisabled: Story = {
  args: {},
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("button", { name: /Undo/ })).toBeDisabled();
    await expect(canvas.getByRole("button", { name: /Redo/ })).toBeDisabled();
  },
};
