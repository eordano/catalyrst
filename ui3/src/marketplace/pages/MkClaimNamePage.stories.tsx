import type { Meta, StoryObj } from "@storybook/react-vite";
import MkClaimNamePage from "./MkClaimNamePage";

const meta = {
  title: "Marketplace/Pages/Claim Name",
  component: MkClaimNamePage,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof MkClaimNamePage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Available: Story = {
  args: { initialName: "genesis", forceStatus: { kind: "available" } },
  render: (args) => <MkClaimNamePage {...args} />,
};

export const Unavailable: Story = {
  args: { initialName: "decentraland", forceStatus: { kind: "unavailable" } },
};

export const InvalidTooShort: Story = {
  args: {
    initialName: "x",
    forceStatus: {
      kind: "invalid",
      warn: true,
      message: "NAME too short: NAMEs must be at least 2 characters long.",
    },
  },
};

export const InsufficientMana: Story = {
  args: {
    initialName: "genesis",
    forceStatus: { kind: "available" },
    insufficientMana: true,
  },
};
