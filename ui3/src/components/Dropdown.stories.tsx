import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import Dropdown from "./Dropdown";

const meta = {
  tags: ["autodocs"],
  title: "Components/Dropdown",
  component: Dropdown,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Dropdown>;

export default meta;
type Story = StoryObj<typeof meta>;

const Frame = (story: ReactNode) => (
  <div style={{ minHeight: "100vh", background: "#0e0e10", padding: 48 }}>
    {story}
  </div>
);

export const Default: Story = {
  render: () => (
    <Dropdown options={["Newest", "Oldest", "Price: Low to High", "Price: High to Low"]} />
  ),
  decorators: [(Story) => Frame(<Story />)],
};

export const WithDefaultValue: Story = {
  render: () => (
    <Dropdown
      options={["All", "Wearables", "Emotes", "Names", "Land"]}
      defaultValue="Emotes"
    />
  ),
  decorators: [(Story) => Frame(<Story />)],
};

export const Controlled: Story = {
  render: () => (
    <Dropdown
      options={["Day", "Week", "Month", "All time"]}
      value="Week"
      onChange={() => {}}
    />
  ),
  decorators: [(Story) => Frame(<Story />)],
};
