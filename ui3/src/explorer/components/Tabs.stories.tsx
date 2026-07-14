import type { Meta, StoryObj } from "@storybook/react-vite";
import Tabs from "./Tabs";

const meta = {
  title: "Explorer/Components/Tabs",
  component: Tabs,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Tabs>;

export default meta;
type Story = StoryObj<typeof meta>;

const tabs = [
  { id: "overview", label: "Overview" },
  { id: "items", label: "Items" },
  { id: "activity", label: "Activity" },
];

export const Default: Story = {
  args: {
    tabs,
    active: "overview",
    variant: "pill",
  },
};

export const Underline: Story = {
  args: {
    tabs,
    active: "items",
    variant: "underline",
  },
};

export const Uncontrolled: Story = {
  args: {
    tabs,
    variant: "pill",
  },
};
