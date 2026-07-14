import type { Meta, StoryObj } from "@storybook/react-vite";
import GvNotFound from "./GvNotFound";

const meta = {
  title: "Governance/Pages/Not Found",
  component: GvNotFound,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvNotFound>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <GvNotFound />,
};

export const ProposalNotFound: Story = {
  render: () => (
    <GvNotFound description="The proposal you are looking for doesn't exist..." />
  ),
};
