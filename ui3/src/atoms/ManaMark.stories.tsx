import type { Meta, StoryObj } from "@storybook/react-vite";
import ManaMark from "./ManaMark";

const meta = {
  title: "Atoms/ManaMark",
  component: ManaMark,
  tags: ["autodocs"],
  parameters: { layout: "centered" },
  argTypes: {
    size: { control: { type: "range", min: 12, max: 96, step: 1 } },
    network: { control: { type: "select" }, options: ["ethereum", "polygon"] },
  },
  args: { size: 16, network: "ethereum" },
} satisfies Meta<typeof ManaMark>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const InBalance: Story = {
  render: (args) => (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        color: "var(--text)",
        font: "600 14px var(--font-sans)",
      }}
    >
      <ManaMark {...args} /> 1,234
    </span>
  ),
};
