import type { Meta, StoryObj } from "@storybook/react-vite";
import SectionIcon from "./SectionIcon";

const meta = {
  title: "Explorer/Atoms/SectionIcon",
  component: SectionIcon,
  parameters: { layout: "centered" },
} satisfies Meta<typeof SectionIcon>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { args: { n: 1 } };

export const Steps: Story = {
  render: () => (
    <div style={{ display: "flex", gap: 16, alignItems: "center", color: "#fff" }}>
      <SectionIcon n={1} validated />
      <SectionIcon n={2} validated />
      <SectionIcon n={3} />
      <SectionIcon n={4} />
    </div>
  ),
};
