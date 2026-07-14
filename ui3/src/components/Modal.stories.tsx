import type { Meta, StoryObj } from "@storybook/react-vite";
import Modal from "./Modal";
import Button from "../atoms/Button";

const meta = {
  tags: ["autodocs"],
  title: "Components/Modal",
  component: Modal,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Modal>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: (args) => (
    <div style={{ minHeight: "100vh", background: "#111" }}>
      <Modal {...args}>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <h2 style={{ margin: 0, fontSize: 20, fontWeight: 800 }}>Modal title</h2>
          <p style={{ margin: 0, color: "var(--ink-6)", lineHeight: 1.5 }}>
            This is the body of the modal. Click the backdrop or press Escape to close.
          </p>
          <Button variant="primary" style={{ alignSelf: "flex-start" }} onClick={() => {}}>
            Confirm
          </Button>
        </div>
      </Modal>
    </div>
  ),
  args: {
    width: 420,
    onClose: () => {},
  },
};

export const Wide: Story = {
  ...Default,
  args: {
    width: 640,
    onClose: () => {},
  },
};

export const Narrow: Story = {
  ...Default,
  args: {
    width: 300,
    onClose: () => {},
  },
};

export const ExplicitAriaLabel: Story = {
  render: (args) => (
    <div style={{ minHeight: "100vh", background: "#111" }}>
      <Modal {...args}>
        <p style={{ margin: 0, color: "var(--ink-6)", lineHeight: 1.5 }}>
          This modal has no heading; its accessible name comes from the ariaLabel prop.
        </p>
      </Modal>
    </div>
  ),
  args: {
    width: 420,
    ariaLabel: "Session expired",
    onClose: () => {},
  },
};
