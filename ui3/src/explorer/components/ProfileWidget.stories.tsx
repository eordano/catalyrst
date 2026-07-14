import type { Decorator, Meta, StoryObj } from "@storybook/react-vite";
import ProfileWidget from "./ProfileWidget";

const meta = {
  title: "Explorer/Components/ProfileWidget",
  component: ProfileWidget,
  args: { open: true },
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ProfileWidget>;

export default meta;
type Story = StoryObj<typeof meta>;

const Backdrop: Decorator = (Story) => (
  <div style={{ minHeight: "100vh", background: "#0d0d12" }}>
    <Story />
  </div>
);

export const Default: Story = {
  decorators: [Backdrop],
  args: {
    name: "Evaristo",
    tag: "#d5f0",
    wallet: "0x23b...7d5f0",
  },
};

export const LongName: Story = {
  decorators: [Backdrop],
  args: {
    name: "Decentralandia",
    tag: "#a1c9",
    wallet: "0x9f4...1a2b3",
  },
};
