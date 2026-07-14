import type { Meta, StoryObj } from "@storybook/react-vite";
import GvDebugAdmin from "./GvDebugAdmin";

const meta = {
  title: "Governance/Pages/Debug Admin",
  component: GvDebugAdmin,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvDebugAdmin>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { authorized: true, version: "v3.41.0" },
};

export const LoginGate: Story = {
  args: { authorized: false },
};
