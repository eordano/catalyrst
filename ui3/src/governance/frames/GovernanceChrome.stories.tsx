import type { Meta, StoryObj } from "@storybook/react-vite";
import GovernanceChrome from "./GovernanceChrome";

const meta = {
  title: "Governance/Frames/GovernanceChrome",
  component: GovernanceChrome,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GovernanceChrome>;

export default meta;
type Story = StoryObj<typeof meta>;

const Body = () => (
  <div style={{ padding: 40, color: "#99939f", fontSize: 14 }}>
    Page body renders here.
  </div>
);

export const Default: Story = {
  render: () => (
    <GovernanceChrome active="proposals">
      <Body />
    </GovernanceChrome>
  ),
};

export const SignedIn: Story = {
  render: () => (
    <GovernanceChrome active="proposals" signedIn>
      <Body />
    </GovernanceChrome>
  ),
};
