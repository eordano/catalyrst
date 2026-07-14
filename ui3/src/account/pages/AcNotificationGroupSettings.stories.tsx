import type { Meta, StoryObj } from "@storybook/react-vite";
import AcNotificationGroupSettings from "./AcNotificationGroupSettings";

const meta = {
  title: "Account/Pages/Notification group settings",
  component: AcNotificationGroupSettings,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof AcNotificationGroupSettings>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    email: "example@decentraland.org",
    hasEmail: true,
    isStreamingEnabled: true,
    isReferralEnabled: true,
  },
};

export const FlagsDisabled: Story = {
  args: {
    email: "example@decentraland.org",
    hasEmail: true,
    isStreamingEnabled: false,
    isReferralEnabled: false,
  },
};

export const NoEmail: Story = {
  args: {
    email: "",
    hasEmail: false,
    isStreamingEnabled: true,
    isReferralEnabled: true,
  },
};
