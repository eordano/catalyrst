import type { Meta, StoryObj } from "@ui/docs/sb";
import { expect, fn, userEvent, waitFor } from "@ui/docs/sb";

import JumpIn from "./JumpIn";
import type { LaunchFn } from "./machine";

const place = {
  id: "plc-genesis",
  title: "Genesis Plaza",
  base_position: "0,0",
  world: false,
  world_name: null,
};

const launch: LaunchFn = async ({ place: p }) => ({
  launchUrl: `https://catalyst.example.com/play/?position=${p.base_position}`,
  realm: "main",
});

const meta = {
  title: "Sites Specs/misc/jump-in/JumpIn",
  component: JumpIn,
  parameters: {
    layout: "centered",
    a11y: { test: "todo" },
  },
  args: {
    place,
    variant: "cta",
    flags: {},
    trackCtx: { sid: "sb-spec", story: "misc/jump-in", variant: "cta" },
    launch,
    track: () => {},
    onLaunch: fn(),
  },
} satisfies Meta<typeof JumpIn>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Idle: Story = {};

export const ConfirmStep: Story = {
  args: { flags: { confirmStep: true } },
  play: async ({ canvas, canvasElement }) => {
    await userEvent.click(await canvas.findByRole("button", { name: "JUMP IN" }));
    await canvas.findAllByRole("button", { name: "CANCEL" });
    await waitFor(() =>
      expect(canvasElement.querySelector(".jumpin__confirm")).toBeTruthy(),
    );
  },
};

export const HappyPath: Story = {
  play: async ({ args, canvas }) => {
    await userEvent.click(await canvas.findByRole("button", { name: "JUMP IN" }));
    await canvas.findByText(/Launching Genesis Plaza/);
    await waitFor(() => expect(args.onLaunch).toHaveBeenCalledTimes(1));
  },
};

export const LaunchError: Story = {
  args: {
    launch: async () => {
      throw new Error("relay unreachable");
    },
  },
  play: async ({ canvas }) => {
    await userEvent.click(await canvas.findByRole("button", { name: "JUMP IN" }));
    await canvas.findByRole("alert");
    await canvas.findByRole("button", { name: "RETRY" });
  },
};
