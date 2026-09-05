import type { Meta, StoryObj } from "@storybook/react-vite";
import AcSignIn from "./AcSignIn";

const meta = {
  title: "Account/Workflows/Sign In",
  component: AcSignIn,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof AcSignIn>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { isConnecting: false, isConnected: false, hasError: false },
};

export const Connecting: Story = {
  args: { isConnecting: true, isConnected: false, hasError: false },
};

export const Error: Story = {
  args: { isConnecting: false, isConnected: false, hasError: true },
};

export const Connected: Story = {
  args: { isConnecting: false, isConnected: true, hasError: false },
};
