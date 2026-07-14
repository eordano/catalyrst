import type { Decorator, Meta, StoryObj } from "@storybook/react-vite";
import { ProfileCardPresentation } from "./ProfileCardPresentation";

const meta = {
  title: "Explorer/Components/ProfileCard",
  component: ProfileCardPresentation,
  parameters: { layout: "fullscreen" },
  args: {
    user: {
      address: "0x23b6f7c1a90d4e5f8a2b1c0d9e8f7a6b5c4d3e2f",
      name: "Morat#1r56",
    },
    x: 120,
    y: 120,
    me: { address: "0x0000000000000000000000000000000000000001" },
    onFriendAction: () => {},
    onMention: () => {},
    onViewPassport: () => {},
    onClose: () => {},
  },
} satisfies Meta<typeof ProfileCardPresentation>;

export default meta;
type Story = StoryObj<typeof meta>;

const Stage: Decorator = (Story) => (
  <div style={{ minHeight: "100vh", background: "#0d0d12" }}>
    <Story />
  </div>
);

export const None: Story = {
  decorators: [Stage],
  args: { relationship: "none" },
};

export const Incoming: Story = {
  decorators: [Stage],
  args: { relationship: "incoming" },
};

export const Requested: Story = {
  decorators: [Stage],
  args: { relationship: "requested" },
};

export const Friend: Story = {
  decorators: [Stage],
  args: {
    relationship: "friend",
    user: { address: "0x23b6f7c1a90d4e5f8a2b1c0d9e8f7a6b5c4d3e2f", name: "Evaristo" },
  },
};

export const Blocked: Story = {
  decorators: [Stage],
  args: { relationship: "blocked" },
};
