import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import Modal from "./Modal";

const meta = {
  title: "Components/Modal/Interactions",
  component: Modal,
  tags: ["autodocs"],
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Modal>;
export default meta;

type Story = StoryObj<typeof meta>;

export const BackdropCloses: Story = {
  args: { ariaLabel: "Demo dialog", onClose: fn(), children: <p>Dialog body</p> },
  play: async ({ args, canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(await body.findByRole("dialog"));
    await expect(args.onClose).not.toHaveBeenCalled();

    const backdrop = canvasElement.ownerDocument.querySelector(".modal__backdrop") as HTMLElement;
    await userEvent.click(backdrop);
    await expect(args.onClose).toHaveBeenCalledTimes(1);
  },
};

export const FocusTrapCyclesTab: Story = {
  args: {
    ariaLabel: "Trap demo",
    onClose: fn(),
    showClose: false,
    children: (
      <>
        <button type="button">First</button>
        <button type="button">Last</button>
      </>
    ),
  },
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    const first = await body.findByRole("button", { name: "First" });
    const last = await body.findByRole("button", { name: "Last" });
    last.focus();
    await userEvent.tab();
    await expect(first).toHaveFocus();
    await userEvent.tab({ shift: true });
    await expect(last).toHaveFocus();
  },
};

export const EscapeCloses: Story = {
  args: { ariaLabel: "Demo dialog", onClose: fn(), children: <p>Dialog body</p> },
  play: async ({ args, canvasElement }) => {
    await within(canvasElement.ownerDocument.body).findByRole("dialog");
    await userEvent.keyboard("{Escape}");
    await expect(args.onClose).toHaveBeenCalledTimes(1);
  },
};
