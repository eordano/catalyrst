import type { Meta, StoryObj } from "@storybook/react-vite";
import AcConvertMANAModal from "./AcConvertMANAModal";

const meta = {
  title: "Account/Components/Convert MANA",
  component: AcConvertMANAModal,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof AcConvertMANAModal>;

export default meta;
type Story = StoryObj<typeof meta>;

export const DepositForm: Story = {
  args: { network: "ethereum", stage: "form" },
};

export const WithdrawCostGate: Story = {
  args: { network: "matic", stage: "cost" },
};

export const WithdrawCostLoading: Story = {
  args: { network: "matic", stage: "cost-loading" },
};

export const WithdrawForm: Story = {
  args: { network: "matic", stage: "form" },
};

export const NoBalanceError: Story = {
  args: { network: "ethereum", stage: "form-error" },
};

export const AuthorizationStep: Story = {
  args: { network: "ethereum", stage: "auth" },
};
