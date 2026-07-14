import type { ReactNode } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import WearablePreview from "../../wearable-preview/WearablePreview";
import Passport from "./Passport";

const meta = {
  title: "Explorer/Pages/Passport",
  component: Passport,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Passport>;

export default meta;
type Story = StoryObj<typeof meta>;

const SHOWCASE_ADDRESS = "0xf12c21d3edb2c0e68935a3bbe5d68ae4bf9dcd7c";
const avatar = (emote = "wave"): ReactNode => (
  <div style={{ width: "100%", height: "100%" }}>
    <WearablePreview profile={SHOWCASE_ADDRESS} emote={emote} zoom={1.1} />
  </div>
);

export const Default: Story = {
  render: () => <Passport avatarPreview={avatar()} />,
};

export const Placeholder: Story = {
  render: () => <Passport />,
};
