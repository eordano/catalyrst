import type { Meta, StoryObj } from "@storybook/react-vite";
import Web3Confirm from "./Web3Confirm";

const meta = {
  title: "Web/Workflows/Web3Confirm",
  component: Web3Confirm,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Web3Confirm>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Web3Confirm />,
};

export const CustomCode: Story = {
  render: () => <Web3Confirm code="482910" expiry="04:37" />,
};

export const Expiring: Story = {
  render: () => <Web3Confirm code="007391" expiry="00:09" />,
};
